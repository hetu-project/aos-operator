use async_trait::async_trait;
use redis::{aio::ConnectionManager, streams::StreamReadOptions, AsyncCommands};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{SystemTime, SystemTimeError, UNIX_EPOCH};
use thiserror::Error;
use tokio::sync::Mutex;

// Define a trait `MessageQueue` for producing, consuming, and acknowledging messages.
// It can be implemented by different backends (like Redis, RabbitMQ, etc.).
#[async_trait]
pub trait MessageQueue {
    type Error;
    type Message;

    // Async function to produce a message to the queue.
    async fn produce(&self, topic: &str, message: &Self::Message) -> Result<(), Self::Error>;

    // Async function to consume messages from the queue.
    async fn consume(&self, topic: &str) -> Result<Vec<Self::Message>, Self::Error>;

    // Async function to acknowledge (delete) a processed message.
    async fn acknowledge(&self, topic: &str, message_id: &str) -> Result<(), Self::Error>;
}

// Define a custom error type `RedisQueueError` that implements the `Error` trait.
// This is done using the `thiserror` macro.
#[derive(Error, Debug)]
pub enum RedisQueueError {
    #[error("Error: redis error: {0}")]
    RedisError(#[from] redis::RedisError),

    #[error("Error: serde_json error: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("Error: system time error: {0}")]
    TimeError(#[from] SystemTimeError),
}

// Define a structure for a message in the Redis queue.
// Each message has an `id` (timestamp) and `data` (the serialized content).
#[derive(Serialize, Deserialize, Debug)]
pub struct RedisMessage {
    pub id: String,   // Unique ID of the message (a timestamp).
    pub data: String, // Message payload serialized as a string.
}

impl RedisMessage {
    // Implement a constructor for `RedisMessage`, which automatically assigns a unique ID (based on system time).
    pub fn new(data: impl Serialize) -> Result<Self, RedisQueueError> {
        Ok(RedisMessage {
            id: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(RedisQueueError::TimeError)?
                .as_millis()
                .to_string()
                + "-0",
            data: serde_json::to_string(&data)?,
        })
    }
}

// Define a struct `RedisStreamPool` to manage Redis connections using a connection pool.
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

// Implement the `MessageQueue` trait for `RedisStreamPool`.
#[async_trait]
impl MessageQueue for RedisStreamPool {
    type Error = RedisQueueError;
    type Message = RedisMessage;

    // Implement the `produce` function to add a message to a Redis stream.
    async fn produce(&self, topic: &str, message: &Self::Message) -> Result<(), Self::Error> {
        let mut conn = self.pool.lock().await;
        let _: () = conn
            .xadd(topic, message.id.as_str(), &[("data", &message.data)])
            .await?;

        Ok(())
    }

    // Implement the `consume` function to read messages from a Redis stream.
    async fn consume(&self, topic: &str) -> Result<Vec<Self::Message>, Self::Error> {
        let mut conn = self.pool.lock().await;

        let options = StreamReadOptions::default().count(1);

        let result: redis::streams::StreamReadReply =
            conn.xread_options(&[topic], &["0"], &options).await?;

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

    // Implement the `acknowledge` function to delete a message from a Redis stream.
    async fn acknowledge(&self, topic: &str, message_id: &str) -> Result<(), Self::Error> {
        let mut conn = self.pool.lock().await;

        conn.xdel(topic, &[message_id]).await?;

        Ok(())
    }
}
