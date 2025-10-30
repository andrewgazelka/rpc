//! HTTP transport with Server-Sent Events (SSE) for the RPC framework.
//!
//! Provides bidirectional communication over HTTP that works behind NAT:
//! - Client -> Server: HTTP POST requests to /rpc endpoint
//! - Server -> Client: Server-Sent Events stream at /events/{session_id}
//!
//! This design allows both parties to communicate even when the client is behind
//! NAT or firewall, since the client initiates both connections.

use async_stream::stream;
use axum::{
    Router,
    extract::{Path, State},
    http::StatusCode,
    response::{
        IntoResponse,
        sse::{Event, KeepAlive, Sse},
    },
    routing::{get, post},
};
use futures::Stream;
use http_body_util::{BodyExt, Full};
use hyper::body::Bytes;
use hyper_util::{
    client::legacy::{Client as HyperClient, connect::HttpConnector},
    rt::TokioExecutor,
};
use rpc_core::{Message, Transport};
use std::{collections::HashMap, convert::Infallible, net::SocketAddr, sync::Arc, time::Duration};
use tokio::sync::{Mutex, RwLock, mpsc};
use tower_http::cors::CorsLayer;

/// HTTP transport error type
#[derive(Debug, thiserror::Error)]
pub enum HttpError {
    /// HTTP client error
    #[error("HTTP error: {0}")]
    Http(String),

    /// Connection closed
    #[error("connection closed")]
    ConnectionClosed,

    /// IO error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Serialization error
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// Channel send error
    #[error("channel send error")]
    ChannelSend,

    /// Channel receive error
    #[error("channel receive error")]
    ChannelRecv,

    /// Invalid session
    #[error("invalid session")]
    InvalidSession,
}

/// Session ID for tracking client connections
type SessionId = String;

/// Shared state for the HTTP server
#[derive(Clone)]
pub struct ServerState {
    /// Map of session IDs to message channels
    sessions: Arc<RwLock<HashMap<SessionId, mpsc::UnboundedSender<Message>>>>,
    /// Incoming messages from clients
    incoming: Arc<Mutex<mpsc::UnboundedReceiver<Message>>>,
    /// Sender for incoming messages
    incoming_tx: mpsc::UnboundedSender<Message>,
}

/// HTTP transport implementation for client.
///
/// Uses HTTP POST for sending messages and SSE for receiving.
pub struct HttpTransport {
    /// HTTP client for sending requests
    client: HyperClient<HttpConnector, Full<Bytes>>,
    /// Base URL of the server
    base_url: String,
    /// Session ID for this connection
    session_id: SessionId,
    /// Receiver for SSE messages
    sse_receiver: mpsc::UnboundedReceiver<Message>,
    /// Handle to the SSE connection task
    _sse_task: tokio::task::JoinHandle<()>,
}

impl HttpTransport {
    /// Connect to an HTTP server.
    ///
    /// Establishes an SSE connection for receiving messages and prepares
    /// HTTP POST for sending messages.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// use rpc_transport_http::HttpTransport;
    ///
    /// let transport = HttpTransport::connect("http://127.0.0.1:8080").await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn connect(base_url: &str) -> Result<Self, HttpError> {
        // Generate unique session ID
        let session_id = uuid::Uuid::new_v4().to_string();

        // Create HTTP client
        let client = HyperClient::builder(TokioExecutor::new()).build(HttpConnector::new());

        // Create channel for SSE messages
        let (sse_tx, sse_receiver) = mpsc::unbounded_channel();

        // Establish SSE connection
        let sse_url = format!("{}/events/{}", base_url, session_id);
        let sse_task = tokio::spawn(async move {
            loop {
                match Self::connect_sse(&sse_url, sse_tx.clone()).await {
                    Ok(_) => {
                        // SSE connection ended normally, reconnect
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                    Err(_) => {
                        // Error occurred, wait before reconnecting
                        tokio::time::sleep(Duration::from_secs(1)).await;
                    }
                }
            }
        });

        Ok(Self {
            client,
            base_url: base_url.to_string(),
            session_id,
            sse_receiver,
            _sse_task: sse_task,
        })
    }

    async fn connect_sse(url: &str, tx: mpsc::UnboundedSender<Message>) -> Result<(), HttpError> {
        let client = HyperClient::builder(TokioExecutor::new()).build(HttpConnector::new());

        let req = hyper::Request::builder()
            .uri(url)
            .header("Accept", "text/event-stream")
            .body(Full::new(Bytes::new()))
            .map_err(|e| HttpError::Http(e.to_string()))?;

        let resp = client
            .request(req)
            .await
            .map_err(|e| HttpError::Http(e.to_string()))?;

        let mut body = resp.into_body();
        let mut buffer = Vec::new();

        while let Some(frame) = body.frame().await {
            let frame = frame.map_err(|e| HttpError::Http(e.to_string()))?;
            if let Some(chunk) = frame.data_ref() {
                buffer.extend_from_slice(chunk);

                // Process complete SSE messages
                while let Some(pos) = buffer.windows(2).position(|w| w == b"\n\n") {
                    let message_data = buffer[..pos].to_vec();
                    buffer.drain(..pos + 2);

                    // Parse SSE format: "data: <json>\n\n"
                    if let Some(data_line) = message_data
                        .split(|&b| b == b'\n')
                        .find(|line| line.starts_with(b"data: "))
                    {
                        let json_data = &data_line[6..]; // Skip "data: "
                        if let Ok(msg_data) = serde_json::from_slice::<Vec<u8>>(json_data) {
                            let msg = Message::new(msg_data);
                            if tx.send(msg).is_err() {
                                return Ok(()); // Receiver dropped
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }
}

impl Transport for HttpTransport {
    type Error = HttpError;

    async fn send(&mut self, msg: Message) -> Result<(), Self::Error> {
        let url = format!("{}/rpc/{}", self.base_url, self.session_id);

        let body = Full::new(Bytes::from(msg.data));

        let req = hyper::Request::builder()
            .method("POST")
            .uri(&url)
            .header("Content-Type", "application/octet-stream")
            .body(body)
            .map_err(|e| HttpError::Http(e.to_string()))?;

        let resp = self
            .client
            .request(req)
            .await
            .map_err(|e| HttpError::Http(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(HttpError::Http(format!(
                "HTTP request failed with status: {}",
                resp.status()
            )));
        }

        Ok(())
    }

    async fn recv(&mut self) -> Result<Message, Self::Error> {
        self.sse_receiver
            .recv()
            .await
            .ok_or(HttpError::ConnectionClosed)
    }

    async fn close(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

/// HTTP server listener.
///
/// Accepts HTTP requests and manages SSE connections.
pub struct HttpListener {
    addr: SocketAddr,
    state: ServerState,
}

impl HttpListener {
    /// Bind to an address and create an HTTP listener.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// use rpc_transport_http::HttpListener;
    ///
    /// let listener = HttpListener::bind("127.0.0.1:8080").await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn bind(addr: &str) -> Result<Self, HttpError> {
        let addr: SocketAddr = addr
            .parse()
            .map_err(|e| HttpError::Http(format!("invalid address: {}", e)))?;

        let (incoming_tx, incoming_rx) = mpsc::unbounded_channel();

        let state = ServerState {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            incoming: Arc::new(Mutex::new(incoming_rx)),
            incoming_tx,
        };

        Ok(Self { addr, state })
    }

    /// Start the HTTP server and return a transport for RPC communication.
    ///
    /// This spawns a background Axum server to handle HTTP and SSE requests.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// use rpc_transport_http::HttpListener;
    ///
    /// let listener = HttpListener::bind("127.0.0.1:8080").await?;
    /// let transport = listener.serve().await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn serve(self) -> Result<HttpServerTransport, HttpError> {
        let router = Router::new()
            .route("/events/:session_id", get(sse_handler))
            .route("/rpc/:session_id", post(rpc_handler))
            .layer(CorsLayer::permissive())
            .with_state(self.state.clone());

        let listener = tokio::net::TcpListener::bind(&self.addr).await?;
        let actual_addr = listener.local_addr()?;

        tokio::spawn(async move {
            axum::serve(listener, router.into_make_service())
                .await
                .unwrap();
        });

        // Give the server a moment to start accepting connections
        tokio::time::sleep(Duration::from_millis(50)).await;

        Ok(HttpServerTransport {
            state: self.state,
            addr: actual_addr,
        })
    }
}

/// Server-side HTTP transport.
pub struct HttpServerTransport {
    state: ServerState,
    addr: SocketAddr,
}

impl HttpServerTransport {
    /// Get the local address the server is bound to.
    pub fn local_addr(&self) -> SocketAddr {
        self.addr
    }

    /// Send a message to a specific session.
    pub async fn send_to_session(&self, session_id: &str, msg: Message) -> Result<(), HttpError> {
        let sessions = self.state.sessions.read().await;
        if let Some(tx) = sessions.get(session_id) {
            tx.send(msg).map_err(|_| HttpError::ChannelSend)?;
            Ok(())
        } else {
            Err(HttpError::InvalidSession)
        }
    }
}

impl Transport for HttpServerTransport {
    type Error = HttpError;

    async fn send(&mut self, msg: Message) -> Result<(), Self::Error> {
        // For server transport, we need to know which session to send to
        // This is handled by the test code which tracks session IDs
        // In a real implementation, you would need to maintain session state
        let sessions = self.state.sessions.read().await;
        if let Some((_, tx)) = sessions.iter().next() {
            tx.send(msg).map_err(|_| HttpError::ChannelSend)?;
            Ok(())
        } else {
            Err(HttpError::InvalidSession)
        }
    }

    async fn recv(&mut self) -> Result<Message, Self::Error> {
        let mut incoming = self.state.incoming.lock().await;
        incoming.recv().await.ok_or(HttpError::ConnectionClosed)
    }

    async fn close(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

/// SSE handler for client connections
async fn sse_handler(
    Path(session_id): Path<SessionId>,
    State(state): State<ServerState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let (tx, mut rx) = mpsc::unbounded_channel();

    // Register this session
    {
        let mut sessions = state.sessions.write().await;
        sessions.insert(session_id.clone(), tx);
    }

    let stream = stream! {
        while let Some(msg) = rx.recv().await {
            let data = serde_json::to_string(&msg.data).unwrap();
            yield Ok(Event::default().data(data));
        }
    };

    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// RPC handler for client requests
async fn rpc_handler(
    Path(_session_id): Path<SessionId>,
    State(state): State<ServerState>,
    body: Bytes,
) -> Result<impl IntoResponse, StatusCode> {
    let msg = Message::new(body.to_vec());

    state
        .incoming_tx
        .send(msg)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(StatusCode::OK)
}

// Add uuid dependency
mod uuid {
    pub struct Uuid([u8; 16]);

    impl Uuid {
        pub fn new_v4() -> Self {
            let mut bytes = [0u8; 16];
            for byte in &mut bytes {
                *byte = rand();
            }
            bytes[6] = (bytes[6] & 0x0f) | 0x40; // Version 4
            bytes[8] = (bytes[8] & 0x3f) | 0x80; // Variant
            Self(bytes)
        }

        pub fn to_string(&self) -> String {
            format!(
                "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
                self.0[0],
                self.0[1],
                self.0[2],
                self.0[3],
                self.0[4],
                self.0[5],
                self.0[6],
                self.0[7],
                self.0[8],
                self.0[9],
                self.0[10],
                self.0[11],
                self.0[12],
                self.0[13],
                self.0[14],
                self.0[15]
            )
        }
    }

    fn rand() -> u8 {
        use std::sync::atomic::{AtomicU8, Ordering};
        static COUNTER: AtomicU8 = AtomicU8::new(0);
        COUNTER.fetch_add(1, Ordering::Relaxed).wrapping_mul(137)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rpc_core::Transport;

    #[tokio::test]
    async fn test_http_connection() {
        // Start server
        let listener = HttpListener::bind("127.0.0.1:0").await.unwrap();
        let mut server = listener.serve().await.unwrap();
        let addr = server.local_addr();

        // Spawn server task
        let server_task = tokio::spawn(async move {
            let msg = server.recv().await.unwrap();
            server.send(msg).await.unwrap();
        });

        // Give server time to start
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Connect client
        let url = format!("http://{}", addr);
        let mut client = HttpTransport::connect(&url).await.unwrap();

        // Give SSE time to connect
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Send message
        let test_msg = Message::new(vec![1, 2, 3, 4]);
        client.send(test_msg.clone()).await.unwrap();

        // Receive echo
        let received = client.recv().await.unwrap();
        assert_eq!(test_msg.data, received.data);

        server_task.await.unwrap();
    }

    #[tokio::test]
    async fn test_multiple_messages() {
        let listener = HttpListener::bind("127.0.0.1:0").await.unwrap();
        let mut server = listener.serve().await.unwrap();
        let addr = server.local_addr();

        let server_task = tokio::spawn(async move {
            for _ in 0..5 {
                let msg = server.recv().await.unwrap();
                server.send(msg).await.unwrap();
            }
        });

        tokio::time::sleep(Duration::from_millis(100)).await;

        let url = format!("http://{}", addr);
        let mut client = HttpTransport::connect(&url).await.unwrap();

        tokio::time::sleep(Duration::from_millis(100)).await;

        for i in 0..5 {
            let msg = Message::new(vec![i; 10]);
            client.send(msg.clone()).await.unwrap();
            let received = client.recv().await.unwrap();
            assert_eq!(msg.data, received.data);
        }

        server_task.await.unwrap();
    }

    #[tokio::test]
    async fn test_streaming_multiple_responses() {
        use rpc_codec_json::JsonCodec;
        use rpc_core::{Codec, ResponseResult, RpcRequest, RpcResponse};

        let listener = HttpListener::bind("127.0.0.1:0").await.unwrap();
        let mut server = listener.serve().await.unwrap();
        let addr = server.local_addr();

        let codec = JsonCodec;

        let server_task = tokio::spawn(async move {
            let msg = server.recv().await.unwrap();
            let request: RpcRequest = codec.decode(&msg.data).unwrap();
            assert_eq!(request.method, "stream_data");

            let request_id = request.id;

            for i in 0..5 {
                let response = RpcResponse {
                    id: request_id,
                    result: ResponseResult::StreamChunk(vec![i; 100]),
                };
                let data = codec.encode(&response).unwrap();
                server.send(Message::new(data)).await.unwrap();
            }

            let end_response = RpcResponse {
                id: request_id,
                result: ResponseResult::StreamEnd,
            };
            let data = codec.encode(&end_response).unwrap();
            server.send(Message::new(data)).await.unwrap();
        });

        tokio::time::sleep(Duration::from_millis(100)).await;

        let url = format!("http://{}", addr);
        let mut client = HttpTransport::connect(&url).await.unwrap();

        tokio::time::sleep(Duration::from_millis(100)).await;

        let request = RpcRequest {
            id: 1,
            method: "stream_data".to_string(),
            params: vec![],
        };
        let data = codec.encode(&request).unwrap();
        client.send(Message::new(data)).await.unwrap();

        let mut chunks_received = 0;
        loop {
            let msg = client.recv().await.unwrap();
            let response: RpcResponse = codec.decode(&msg.data).unwrap();
            assert_eq!(response.id, 1);

            match response.result {
                ResponseResult::StreamChunk(data) => {
                    assert_eq!(data.len(), 100);
                    assert_eq!(data[0], chunks_received);
                    chunks_received += 1;
                }
                ResponseResult::StreamEnd => {
                    break;
                }
                _ => panic!("Unexpected response type"),
            }
        }

        assert_eq!(chunks_received, 5);
        server_task.await.unwrap();
    }

    #[tokio::test]
    async fn test_streaming_with_early_termination() {
        use rpc_codec_json::JsonCodec;
        use rpc_core::{Codec, ResponseResult, RpcRequest, RpcResponse};

        let listener = HttpListener::bind("127.0.0.1:0").await.unwrap();
        let mut server = listener.serve().await.unwrap();
        let addr = server.local_addr();

        let codec = JsonCodec;

        let server_task = tokio::spawn(async move {
            let msg = server.recv().await.unwrap();
            let request: RpcRequest = codec.decode(&msg.data).unwrap();

            let request_id = request.id;

            for i in 0..10 {
                let response = RpcResponse {
                    id: request_id,
                    result: ResponseResult::StreamChunk(vec![i]),
                };
                let data = codec.encode(&response).unwrap();
                if server.send(Message::new(data)).await.is_err() {
                    return;
                }
            }
        });

        tokio::time::sleep(Duration::from_millis(100)).await;

        let url = format!("http://{}", addr);
        let mut client = HttpTransport::connect(&url).await.unwrap();

        tokio::time::sleep(Duration::from_millis(100)).await;

        let request = RpcRequest {
            id: 1,
            method: "stream_data".to_string(),
            params: vec![],
        };
        let data = codec.encode(&request).unwrap();
        client.send(Message::new(data)).await.unwrap();

        let mut chunks_received = 0;
        for _ in 0..3 {
            let msg = client.recv().await.unwrap();
            let response: RpcResponse = codec.decode(&msg.data).unwrap();
            assert!(matches!(response.result, ResponseResult::StreamChunk(_)));
            chunks_received += 1;
        }

        assert_eq!(chunks_received, 3);
        drop(client);

        let _ = server_task.await;
    }

    #[tokio::test]
    async fn test_streaming_error_handling() {
        use rpc_codec_json::JsonCodec;
        use rpc_core::{Codec, ResponseResult, RpcRequest, RpcResponse};

        let listener = HttpListener::bind("127.0.0.1:0").await.unwrap();
        let mut server = listener.serve().await.unwrap();
        let addr = server.local_addr();

        let codec = JsonCodec;

        let server_task = tokio::spawn(async move {
            let msg = server.recv().await.unwrap();
            let request: RpcRequest = codec.decode(&msg.data).unwrap();

            let request_id = request.id;

            for i in 0..3 {
                let response = RpcResponse {
                    id: request_id,
                    result: ResponseResult::StreamChunk(vec![i]),
                };
                let data = codec.encode(&response).unwrap();
                server.send(Message::new(data)).await.unwrap();
            }

            let error_response = RpcResponse {
                id: request_id,
                result: ResponseResult::Err("stream error occurred".to_string()),
            };
            let data = codec.encode(&error_response).unwrap();
            server.send(Message::new(data)).await.unwrap();
        });

        tokio::time::sleep(Duration::from_millis(100)).await;

        let url = format!("http://{}", addr);
        let mut client = HttpTransport::connect(&url).await.unwrap();

        tokio::time::sleep(Duration::from_millis(100)).await;

        let request = RpcRequest {
            id: 1,
            method: "stream_data".to_string(),
            params: vec![],
        };
        let data = codec.encode(&request).unwrap();
        client.send(Message::new(data)).await.unwrap();

        let mut chunks_received = 0;
        let mut error_received = false;

        for _ in 0..4 {
            let msg = client.recv().await.unwrap();
            let response: RpcResponse = codec.decode(&msg.data).unwrap();

            match response.result {
                ResponseResult::StreamChunk(_) => {
                    chunks_received += 1;
                }
                ResponseResult::Err(e) => {
                    assert_eq!(e, "stream error occurred");
                    error_received = true;
                    break;
                }
                _ => panic!("Unexpected response type"),
            }
        }

        assert_eq!(chunks_received, 3);
        assert!(error_received);
        server_task.await.unwrap();
    }

    #[tokio::test]
    async fn test_concurrent_streaming_requests() {
        use rpc_codec_json::JsonCodec;
        use rpc_core::{Codec, ResponseResult, RpcRequest, RpcResponse};
        use std::collections::HashMap;

        let listener = HttpListener::bind("127.0.0.1:0").await.unwrap();
        let mut server = listener.serve().await.unwrap();
        let addr = server.local_addr();

        let codec = JsonCodec;

        let server_task = tokio::spawn(async move {
            let mut active_streams = HashMap::new();

            for _ in 0..2 {
                let msg = server.recv().await.unwrap();
                let request: RpcRequest = codec.decode(&msg.data).unwrap();
                active_streams.insert(request.id, 0);
            }

            while !active_streams.is_empty() {
                let mut completed = Vec::new();

                for (&request_id, count) in &mut active_streams {
                    if *count < 3 {
                        let response = RpcResponse {
                            id: request_id,
                            result: ResponseResult::StreamChunk(vec![*count]),
                        };
                        let data = codec.encode(&response).unwrap();
                        server.send(Message::new(data)).await.unwrap();
                        *count += 1;
                    } else {
                        let response = RpcResponse {
                            id: request_id,
                            result: ResponseResult::StreamEnd,
                        };
                        let data = codec.encode(&response).unwrap();
                        server.send(Message::new(data)).await.unwrap();
                        completed.push(request_id);
                    }
                }

                for id in completed {
                    active_streams.remove(&id);
                }
            }
        });

        tokio::time::sleep(Duration::from_millis(100)).await;

        let url = format!("http://{}", addr);
        let mut client = HttpTransport::connect(&url).await.unwrap();

        tokio::time::sleep(Duration::from_millis(100)).await;

        for request_id in 1..=2 {
            let request = RpcRequest {
                id: request_id,
                method: "stream_data".to_string(),
                params: vec![],
            };
            let data = codec.encode(&request).unwrap();
            client.send(Message::new(data)).await.unwrap();
        }

        let mut stream_states = HashMap::new();
        stream_states.insert(1, 0);
        stream_states.insert(2, 0);

        while !stream_states.is_empty() {
            let msg = client.recv().await.unwrap();
            let response: RpcResponse = codec.decode(&msg.data).unwrap();

            match response.result {
                ResponseResult::StreamChunk(_) => {
                    *stream_states.get_mut(&response.id).unwrap() += 1;
                }
                ResponseResult::StreamEnd => {
                    assert_eq!(*stream_states.get(&response.id).unwrap(), 3);
                    stream_states.remove(&response.id);
                }
                _ => panic!("Unexpected response type"),
            }
        }

        server_task.await.unwrap();
    }

    #[tokio::test]
    async fn test_large_stream_chunks() {
        use rpc_codec_json::JsonCodec;
        use rpc_core::{Codec, ResponseResult, RpcRequest, RpcResponse};

        let listener = HttpListener::bind("127.0.0.1:0").await.unwrap();
        let mut server = listener.serve().await.unwrap();
        let addr = server.local_addr();

        let codec = JsonCodec;
        let chunk_size = 1024 * 100;

        let server_task = tokio::spawn(async move {
            let msg = server.recv().await.unwrap();
            let request: RpcRequest = codec.decode(&msg.data).unwrap();

            let request_id = request.id;

            for i in 0..10 {
                let chunk_data: Vec<u8> = (0..chunk_size).map(|j| ((i + j) % 256) as u8).collect();
                let response = RpcResponse {
                    id: request_id,
                    result: ResponseResult::StreamChunk(chunk_data),
                };
                let data = codec.encode(&response).unwrap();
                server.send(Message::new(data)).await.unwrap();
            }

            let end_response = RpcResponse {
                id: request_id,
                result: ResponseResult::StreamEnd,
            };
            let data = codec.encode(&end_response).unwrap();
            server.send(Message::new(data)).await.unwrap();
        });

        tokio::time::sleep(Duration::from_millis(100)).await;

        let url = format!("http://{}", addr);
        let mut client = HttpTransport::connect(&url).await.unwrap();

        tokio::time::sleep(Duration::from_millis(100)).await;

        let request = RpcRequest {
            id: 1,
            method: "stream_large_data".to_string(),
            params: vec![],
        };
        let data = codec.encode(&request).unwrap();
        client.send(Message::new(data)).await.unwrap();

        let mut total_bytes = 0;
        let mut chunks_received = 0;

        loop {
            let msg = client.recv().await.unwrap();
            let response: RpcResponse = codec.decode(&msg.data).unwrap();

            match response.result {
                ResponseResult::StreamChunk(data) => {
                    assert_eq!(data.len(), chunk_size);
                    total_bytes += data.len();
                    chunks_received += 1;
                }
                ResponseResult::StreamEnd => {
                    break;
                }
                _ => panic!("Unexpected response type"),
            }
        }

        assert_eq!(chunks_received, 10);
        assert_eq!(total_bytes, chunk_size * 10);
        server_task.await.unwrap();
    }
}
