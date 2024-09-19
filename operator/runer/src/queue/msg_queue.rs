use async_trait::async_trait;
use redis::aio::ConnectionManager;
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio;
use tokio::sync::Mutex;
use tracing::*;

#[async_trait]
pub trait MessageQueue {
    type Error;
    type Message;

    async fn produce(&self, topic: &str, message: &Self::Message) -> Result<(), Self::Error>;

    async fn consume(&self, topic: &str) -> Result<Vec<Self::Message>, Self::Error>;

    async fn acknowledge(&mut self, topic: &str, message_id: &str) -> Result<(), Self::Error>;
}

#[derive(Debug)]
pub enum RedisQueueError {
    RedisError(redis::RedisError),
    JsonError(serde_json::Error),
}

impl From<redis::RedisError> for RedisQueueError {
    fn from(error: redis::RedisError) -> Self {
        RedisQueueError::RedisError(error)
    }
}

impl From<serde_json::Error> for RedisQueueError {
    fn from(error: serde_json::Error) -> Self {
        RedisQueueError::JsonError(error)
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct RedisMessage {
    pub id: String,
    pub data: String,
}

impl From<&str> for RedisMessage {
    fn from(data: &str) -> Self {
        RedisMessage {
            id: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis()
                .to_string()
                + "-0",
            data: data.to_owned(),
        }
    }
}

impl From<String> for RedisMessage {
    fn from(data: String) -> Self {
        RedisMessage {
            id: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis()
                .to_string()
                + "-0",
            data,
        }
    }
}

impl From<serde_json::Value> for RedisMessage {
    fn from(data: serde_json::Value) -> Self {
        RedisMessage {
            id: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis()
                .to_string()
                + "-0",
            data: data.as_str().unwrap().to_owned(),
        }
    }
}

impl RedisMessage {
    pub fn new(data: impl Serialize) -> Self {
        RedisMessage {
            id: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis()
                .to_string()
                + "-0",
            data: serde_json::to_string(&data).unwrap(),
        }
    }
}

#[derive(Clone)]
pub struct RedisStreamPool {
    pool: Arc<Mutex<ConnectionManager>>,
}

impl RedisStreamPool {
    pub async fn new(redis_url: &str) -> Result<Self, RedisQueueError> {
        let client = redis::Client::open(redis_url)?;
        let manager = ConnectionManager::new(client).await?;
        Ok(RedisStreamPool {
            pool: Arc::new(Mutex::new(manager)),
        })
    }
}

#[async_trait]
impl MessageQueue for RedisStreamPool {
    type Error = RedisQueueError;
    type Message = RedisMessage;
    //type Message = serde_json::Value;

    async fn produce(&self, topic: &str, message: &Self::Message) -> Result<(), Self::Error> {
        let mut conn = self.pool.lock().await;
        let _: () = conn
            .xadd(topic, message.id.as_str(), &[("data", &message.data)])
            .await?;

        Ok(())
    }

    async fn consume(&self, topic: &str) -> Result<Vec<Self::Message>, Self::Error> {
        let mut conn = self.pool.lock().await;

        let result: redis::streams::StreamReadReply = conn.xread(&[topic], &["0"]).await?;

        if let Some(stream_key) = result.keys.get(0) {
            let message_values: Vec<_> = stream_key
                .ids
                .iter()
                .filter_map(|vs| {
                    vs.get("data").map(|v: String| {
                        let v_clone = v.clone();
                        RedisMessage {
                            id: vs.id.clone(),
                            data: Into::<String>::into(v_clone),
                        }
                    })
                })
                .collect();
            return Ok(message_values);
        }

        Ok(vec![])
    }

    async fn acknowledge(&mut self, topic: &str, message_id: &str) -> Result<(), Self::Error> {
        let mut conn = self.pool.lock().await;

        conn.xdel(topic, &[message_id]).await?;

        Ok(())
    }
}
