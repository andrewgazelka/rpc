//! WebSocket transport for the RPC framework.
//!
//! Provides WebSocket-based transport implementation using tokio-tungstenite.

use futures::{SinkExt, StreamExt};
use rpc_core::{Error, Message, Result, Transport};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::{
    accept_async, connect_async, tungstenite::protocol::Message as WsMessage, MaybeTlsStream,
    WebSocketStream,
};

/// WebSocket transport implementation.
///
/// Can be used for both client and server connections.
pub struct WebSocketTransport {
    ws: WebSocketStream<MaybeTlsStream<TcpStream>>,
}

/// WebSocket transport for server-side connections (no TLS wrapping needed)
pub struct WebSocketServerTransport {
    ws: WebSocketStream<TcpStream>,
}

impl WebSocketTransport {
    /// Connect to a WebSocket server.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example() -> rpc_core::Result<()> {
    /// use rpc_transport_ws::WebSocketTransport;
    ///
    /// let transport = WebSocketTransport::connect("ws://127.0.0.1:8080").await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn connect(url: &str) -> Result<Self> {
        let (ws, _) = connect_async(url)
            .await
            .map_err(|e| Error::transport(format!("WebSocket connect error: {}", e)))?;

        Ok(Self { ws })
    }
}

impl Transport for WebSocketTransport {
    async fn send(&mut self, msg: Message) -> Result<()> {
        self.ws
            .send(WsMessage::Binary(msg.data))
            .await
            .map_err(|e| Error::transport(format!("WebSocket send error: {}", e)))
    }

    async fn recv(&mut self) -> Result<Message> {
        match self.ws.next().await {
            Some(Ok(WsMessage::Binary(data))) => Ok(Message::new(data)),
            Some(Ok(WsMessage::Close(_))) => Err(Error::ConnectionClosed),
            Some(Ok(_)) => Err(Error::transport("unexpected message type")),
            Some(Err(e)) => Err(Error::transport(format!("WebSocket recv error: {}", e))),
            None => Err(Error::ConnectionClosed),
        }
    }

    async fn close(&mut self) -> Result<()> {
        self.ws
            .close(None)
            .await
            .map_err(|e| Error::transport(format!("WebSocket close error: {}", e)))
    }
}

/// WebSocket server listener.
///
/// Accepts incoming WebSocket connections and creates transports.
pub struct WebSocketListener {
    pub listener: TcpListener,
}

impl WebSocketListener {
    /// Bind to an address and create a WebSocket listener.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # async fn example() -> rpc_core::Result<()> {
    /// use rpc_transport_ws::WebSocketListener;
    ///
    /// let listener = WebSocketListener::bind("127.0.0.1:8080").await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn bind(addr: &str) -> Result<Self> {
        let listener = TcpListener::bind(addr)
            .await
            .map_err(|e| Error::transport(format!("Failed to bind: {}", e)))?;

        Ok(Self { listener })
    }

    /// Accept an incoming connection and create a WebSocket transport.
    pub async fn accept(&self) -> Result<WebSocketServerTransport> {
        let (stream, _) = self
            .listener
            .accept()
            .await
            .map_err(|e| Error::transport(format!("Failed to accept: {}", e)))?;

        WebSocketServerTransport::from_stream(stream).await
    }
}

impl WebSocketServerTransport {
    /// Create a WebSocket transport from an accepted connection.
    ///
    /// Used on the server side after accepting a TCP connection.
    pub async fn from_stream(stream: TcpStream) -> Result<Self> {
        let ws = accept_async(stream)
            .await
            .map_err(|e| Error::transport(format!("WebSocket accept error: {}", e)))?;

        Ok(Self { ws })
    }
}

impl Transport for WebSocketServerTransport {
    async fn send(&mut self, msg: Message) -> Result<()> {
        self.ws
            .send(WsMessage::Binary(msg.data))
            .await
            .map_err(|e| Error::transport(format!("WebSocket send error: {}", e)))
    }

    async fn recv(&mut self) -> Result<Message> {
        match self.ws.next().await {
            Some(Ok(WsMessage::Binary(data))) => Ok(Message::new(data)),
            Some(Ok(WsMessage::Close(_))) => Err(Error::ConnectionClosed),
            Some(Ok(_)) => Err(Error::transport("unexpected message type")),
            Some(Err(e)) => Err(Error::transport(format!("WebSocket recv error: {}", e))),
            None => Err(Error::ConnectionClosed),
        }
    }

    async fn close(&mut self) -> Result<()> {
        self.ws
            .close(None)
            .await
            .map_err(|e| Error::transport(format!("WebSocket close error: {}", e)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rpc_core::Transport;

    #[tokio::test]
    async fn test_websocket_connection() {
        // Start a server
        let listener = WebSocketListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.listener.local_addr().unwrap();

        // Spawn server task
        let server_task = tokio::spawn(async move {
            let mut transport = listener.accept().await.unwrap();
            let msg = transport.recv().await.unwrap();
            transport.send(msg).await.unwrap();
        });

        // Connect client
        let url = format!("ws://{}", addr);
        let mut client = WebSocketTransport::connect(&url).await.unwrap();

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
        let listener = WebSocketListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.listener.local_addr().unwrap();

        let server_task = tokio::spawn(async move {
            let mut transport = listener.accept().await.unwrap();
            for _ in 0..5 {
                let msg = transport.recv().await.unwrap();
                transport.send(msg).await.unwrap();
            }
        });

        let url = format!("ws://{}", addr);
        let mut client = WebSocketTransport::connect(&url).await.unwrap();

        for i in 0..5 {
            let msg = Message::new(vec![i; 10]);
            client.send(msg.clone()).await.unwrap();
            let received = client.recv().await.unwrap();
            assert_eq!(msg.data, received.data);
        }

        server_task.await.unwrap();
    }
}
