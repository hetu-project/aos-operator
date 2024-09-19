#[cfg(test)]
mod test;

use futures::{SinkExt, StreamExt};
use log::info;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::Error;
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;
use tokio_tungstenite::accept_async;
use tokio_tungstenite::tungstenite::protocol::Message;
use url::Url;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireMessage {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    pub address: String,
    pub hash: String,
    pub signature: String,
}

impl WireMessage {
    /// Deserialize a WireMessage through serde_json
    fn try_from_bytes(b: Vec<u8>) -> std::io::Result<Self> {
        let w: WireMessage = serde_json::from_slice(&b)?;
        Ok(w)
    }

    /// Create a new request message with serde_json message (with new unique msg id).
    fn request(f: String, s: Value) -> std::io::Result<(Message, String)> {
        static ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let id = ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let s1 = Self {
            id: id.to_string(),
            method: Some(f),
            params: Some(s),
            result: None,
            address: "".to_string(),
            hash: "".to_string(),
            signature: "".to_string(),
        };
        let s2 = serde_json::to_string(&s1).map_err(Error::other)?;
        Ok((Message::Text(s2), id.to_string()))
    }

    /// Create a new response message.
    fn response(id: String, s: Value) -> std::io::Result<Message> {
        let s1 = Self {
            id: id,
            method: None,
            params: None,
            result: Some(s),
            address: "".to_string(),
            hash: "".to_string(),
            signature: "".to_string(),
        };
        let s2 = serde_json::to_string(&s1).map_err(Error::other)?;
        Ok(Message::Text(s2))
    }
}

pub struct WebsocketRespond {
    id: String,
    core: WsCoreSync,
}

impl std::fmt::Debug for WebsocketRespond {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WebsocketRespond")
            .field("id", &self.id)
            .finish()
    }
}

impl WebsocketRespond {
    /// Respond to an incoming request.
    pub async fn respond(self, s: Value) -> std::io::Result<()> {
        use futures::sink::SinkExt;
        self.core
            .exec(move |_, core| async move {
                tokio::time::timeout(core.timeout, async {
                    let s = WireMessage::response(self.id, s)?;
                    core.send.lock().await.send(s).await.map_err(Error::other)?;
                    Ok(())
                })
                .await
                .map_err(Error::other)?
            })
            .await
    }
}

/// Types of messages that can be received by a WebsocketReceiver.
#[derive(Debug)]
pub enum ReceiveMessage {
    /// Received a signal from the remote.
    Signal(Vec<u8>),

    /// Received a request from the remote.
    Request(String, String, Value, WebsocketRespond),
}

pub struct WebsocketListener {
    config: Arc<WebsocketConfig>,
    pub listener: tokio::net::TcpListener,
}

impl WebsocketListener {
    /// Get the bound local address of this listener.
    pub fn local_addr(&self) -> std::io::Result<std::net::SocketAddr> {
        self.listener.local_addr()
    }

    /// Bind a new websocket listener.
    pub async fn bind<A: tokio::net::ToSocketAddrs>(
        config: Arc<WebsocketConfig>,
        addr: A,
    ) -> std::io::Result<Self> {
        let listener = tokio::net::TcpListener::bind(addr).await?;

        let addr = listener.local_addr()?;
        info!("WebsocketListener Listening {}", addr);
        Ok(Self { config, listener })
    }

    /// accept incoming connection for server
    pub async fn accept(&self) -> std::io::Result<(WebsocketSender, WebsocketReceiver)> {
        let (stream, addr) = self.listener.accept().await?;
        info!("Accept Incoming Websocket Connection");
        let stream =
            tokio_tungstenite::accept_async_with_config(stream, Some(self.config.to_tungstenite()))
                .await
                .map_err(Error::other)?;
        split(stream, self.config.default_request_timeout, addr)
    }
}

#[derive(Clone)]
pub struct WebsocketSender(WsCoreSync, std::time::Duration);

impl WebsocketSender {
    pub async fn request(&self, f: String, s: Value) -> std::io::Result<Value> {
        self.request_timeout(f, s, self.1).await
    }

    pub async fn request_timeout(
        &self,
        f: String,
        s: Value,
        timeout: std::time::Duration,
    ) -> std::io::Result<Value> {
        let timeout_at = tokio::time::Instant::now() + timeout;
        use futures::sink::SinkExt;

        let (s, id) = WireMessage::request(f, s)?;

        /// Drop helper to remove our response callback if we timeout.
        struct D(CallbackMap, String);

        impl Drop for D {
            fn drop(&mut self) {
                self.0.remove(self.1.clone());
            }
        }

        let (resp_s, resp_r) = tokio::sync::oneshot::channel();

        let _drop = self
            .0
            .exec(move |_, core| async move {
                // create the drop helper
                let drop = D(core.callback.clone(), id.clone());

                // register the response callback
                core.callback.insert(id, resp_s);

                tokio::time::timeout_at(timeout_at, async move {
                    // send the actual message
                    //info!("-->raw msg: {s:?}");
                    core.send.lock().await.send(s).await.map_err(Error::other)?;

                    Ok(drop)
                })
                .await
                .map_err(Error::other)?
            })
            .await?;

        tokio::time::timeout_at(timeout_at, async {
            // await the response
            let resp = resp_r
                .await
                .map_err(|_| Error::other("ResponderDropped"))??;

            Ok(resp)
        })
        .await
        .map_err(Error::other)?
    }
}

#[allow(dead_code)]
pub struct WebsocketReceiver(
    WsCoreSync,
    std::net::SocketAddr,
    tokio::task::JoinHandle<()>,
);

impl WebsocketReceiver {
    fn new(core: WsCoreSync, addr: std::net::SocketAddr) -> Self {
        let core2 = core.clone();
        let ping_task = tokio::task::spawn(async move {
            /*
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                let core = core2.0.lock().unwrap().as_ref().cloned();
                if let Some(core) = core {
                    use futures::sink::SinkExt;
                    if core
                        .send
                        .lock()
                        .await
                        .send(Message::Ping(Vec::new()))
                        .await
                        .is_err()
                    {
                        core2.close();
                    }
                } else {
                    break;
                }
            }
            */
        });
        Self(core, addr, ping_task)
    }

    /// Peer address.
    pub fn peer_addr(&self) -> std::net::SocketAddr {
        self.1
    }

    /// Receive the next message.
    pub async fn recv(&mut self) -> std::io::Result<ReceiveMessage> {
        match self.recv_inner().await {
            Err(err) => {
                info!("WebsocketReceiver Error: {:?}", err);
                Err(err)
            }
            Ok(msg) => Ok(msg),
        }
    }

    async fn recv_inner(&mut self) -> std::io::Result<ReceiveMessage> {
        use futures::sink::SinkExt;
        use futures::stream::StreamExt;
        loop {
            if let Some(result) = self
                .0
                .exec(move |core_sync, core| async move {
                    let msg = core
                        .recv
                        .lock()
                        .await
                        .next()
                        .await
                        .ok_or(Error::other("ReceiverClosed"))?
                        .map_err(Error::other)?;
                    let msg = match msg {
                        Message::Text(s) => s.into_bytes(),
                        Message::Binary(b) => b,
                        Message::Ping(b) => {
                            core.send
                                .lock()
                                .await
                                .send(Message::Pong(b))
                                .await
                                .map_err(Error::other)?;
                            return Ok(None);
                        }
                        Message::Pong(_) => return Ok(None),
                        Message::Close(frame) => {
                            return Err(Error::other(format!("ReceivedCloseFrame: {frame:?}")));
                        }
                        Message::Frame(_) => return Err(Error::other("UnexpectedRawFrame")),
                    };
                    //info!( "<--raw msg: {:?}-{:?}", WireMessage::try_from_bytes(msg.clone())?, msg.clone());
                    let recvd = WireMessage::try_from_bytes(msg)?;
                    match recvd.method {
                        Some(meth) => {
                            let resp = WebsocketRespond {
                                id: recvd.id.clone(),
                                core: core_sync,
                            };
                            Ok(Some(ReceiveMessage::Request(
                                recvd.id,
                                meth,
                                recvd.params.unwrap(),
                                resp,
                            )))
                        }
                        None => {
                            if let Some(sender) = core.callback.remove(recvd.id) {
                                if let Some(data) = recvd.result {
                                    let _ = sender.send(Ok(data));
                                }
                            }
                            Ok(None)
                        }
                    }
                })
                .await?
            {
                return Ok(result);
            }
        }
    }
}

type WsSendSync = Arc<
    tokio::sync::Mutex<
        futures::stream::SplitSink<
            tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
            tokio_tungstenite::tungstenite::protocol::Message,
        >,
    >,
>;
type WsRecvSync = Arc<
    tokio::sync::Mutex<
        futures::stream::SplitStream<tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>>,
    >,
>;

#[derive(Clone)]
#[allow(clippy::type_complexity)]
struct CallbackMap(
    Arc<
        std::sync::Mutex<
            std::collections::HashMap<String, tokio::sync::oneshot::Sender<std::io::Result<Value>>>,
        >,
    >,
);

impl CallbackMap {
    pub fn close(&self) {
        if let Ok(mut lock) = self.0.lock() {
            for (_, s) in lock.drain() {
                let _ = s.send(Err(Error::other("ConnectionClosed")));
            }
        }
    }

    pub fn insert(&self, id: String, sender: tokio::sync::oneshot::Sender<std::io::Result<Value>>) {
        self.0.lock().unwrap().insert(id, sender);
    }

    pub fn remove(
        &self,
        id: String,
    ) -> Option<tokio::sync::oneshot::Sender<std::io::Result<Value>>> {
        self.0.lock().unwrap().remove(&id)
    }
}

#[derive(Clone)]
struct WsCoreSync(Arc<std::sync::Mutex<Option<WsCore>>>);

impl WsCoreSync {
    fn close(&self) {
        if let Some(core) = self.0.lock().unwrap().take() {
            core.callback.close();
            tokio::task::spawn(async move {
                use futures::sink::SinkExt;
                let _ = core.send.lock().await.close().await;
            });
        }
    }

    fn close_if_err<R>(&self, r: std::io::Result<R>) -> std::io::Result<R> {
        match r {
            Err(err) => {
                self.close();
                Err(err)
            }
            Ok(res) => Ok(res),
        }
    }

    pub async fn exec<F, C, R>(&self, c: C) -> std::io::Result<R>
    where
        F: std::future::Future<Output = std::io::Result<R>>,
        C: FnOnce(WsCoreSync, WsCore) -> F,
    {
        let core = match self.0.lock().unwrap().as_ref() {
            Some(core) => core.clone(),
            None => return Err(Error::other("WebsocketClosed")),
        };
        self.close_if_err(c(self.clone(), core).await)
    }
}

#[derive(Clone)]
struct WsCore {
    pub send: WsSendSync,
    pub recv: WsRecvSync,
    pub callback: CallbackMap,
    pub timeout: std::time::Duration,
}

/// can be used both client and server
fn split(
    stream: tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
    timeout: std::time::Duration,
    peer_addr: std::net::SocketAddr,
) -> std::io::Result<(WebsocketSender, WebsocketReceiver)> {
    let (sink, stream) = futures::stream::StreamExt::split(stream);

    let core = WsCore {
        send: Arc::new(tokio::sync::Mutex::new(sink)),
        recv: Arc::new(tokio::sync::Mutex::new(stream)),
        callback: CallbackMap(Arc::new(std::sync::Mutex::new(
            std::collections::HashMap::new(),
        ))),
        timeout,
    };

    let core_send = WsCoreSync(Arc::new(std::sync::Mutex::new(Some(core))));
    let core_recv = core_send.clone();

    Ok((
        WebsocketSender(core_send, timeout),
        WebsocketReceiver::new(core_recv, peer_addr),
    ))
}

pub async fn listening(
    listener: tokio::net::TcpListener,
) -> Result<(), Box<dyn std::error::Error>> {
    while let Ok((stream, _)) = listener.accept().await {
        let ws_stream = accept_async(stream).await?;
        tokio::spawn(async move {
            handle_connection(ws_stream).await;
        });
    }
    Ok(())
}

async fn handle_connection(stream: tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>) {
    let (mut sender, mut receiver) = stream.split();

    while let Some(Ok(msg)) = receiver.next().await {
        println!("Received message: {:?}", msg);

        if let Err(e) = sender.send(Message::Text(msg.to_string())).await {
            println!("Failed to send message: {:?}", e);
            return;
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct WebsocketConfig {
    /// Seconds after which the lib will stop tracking individual request ids.
    /// [default = 60 seconds]
    pub default_request_timeout: std::time::Duration,

    /// Maximum total message size of a websocket message. [default = 64M]
    pub max_message_size: usize,

    /// Maximum websocket frame size. [default = 16M]
    pub max_frame_size: usize,
}

impl WebsocketConfig {
    /// The default WebsocketConfig.
    pub const DEFAULT: WebsocketConfig = WebsocketConfig {
        default_request_timeout: std::time::Duration::from_secs(60),
        max_message_size: 64 << 20,
        max_frame_size: 16 << 20,
    };

    /// Internal convert to tungstenite config.
    pub(crate) fn to_tungstenite(
        self,
    ) -> tokio_tungstenite::tungstenite::protocol::WebSocketConfig {
        tokio_tungstenite::tungstenite::protocol::WebSocketConfig {
            max_message_size: Some(self.max_message_size),
            max_frame_size: Some(self.max_frame_size),
            ..Default::default()
        }
    }
}

impl Default for WebsocketConfig {
    fn default() -> Self {
        WebsocketConfig::DEFAULT
    }
}

pub async fn connect(
    config: Arc<WebsocketConfig>,
    addr: std::net::SocketAddr,
) -> std::io::Result<(WebsocketSender, WebsocketReceiver)> {
    let stream = tokio::net::TcpStream::connect(addr).await?;
    let peer_addr = stream.peer_addr()?;
    let url = format!("ws://{addr}/ws");
    info!("ws url: {:?}", url);
    let (stream, _addr) =
        tokio_tungstenite::client_async_with_config(url, stream, Some(config.to_tungstenite()))
            .await
            .map_err(Error::other)?;
    split(stream, config.default_request_timeout, peer_addr)
}
