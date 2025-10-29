//! In-process transport for RPC framework.
//!
//! This transport uses tokio channels for communication within the same process.
//! Ideal for:
//! - Testing RPC systems without network overhead
//! - Embedding RPC server in the same binary as client
//! - Plugin architectures where plugins communicate via RPC
//! - Hot-reloadable dynamic libraries

use rpc_core::{Message, Transport};
use tokio::sync::mpsc;

/// In-process transport error type
#[derive(Debug, thiserror::Error)]
pub enum InProcessError {
    /// Channel closed
    #[error("channel closed")]
    ChannelClosed,
}

/// In-process transport using tokio channels.
///
/// Messages are passed directly through memory channels with no serialization
/// overhead at the transport layer (serialization still happens at codec layer).
pub struct InProcessTransport {
    sender: mpsc::UnboundedSender<Message>,
    receiver: mpsc::UnboundedReceiver<Message>,
}

impl InProcessTransport {
    /// Create a pair of connected in-process transports.
    ///
    /// Returns (client_transport, server_transport) where messages sent
    /// on one side are received on the other.
    pub fn pair() -> (Self, Self) {
        let (tx1, rx1) = mpsc::unbounded_channel();
        let (tx2, rx2) = mpsc::unbounded_channel();

        let t1 = InProcessTransport {
            sender: tx2,
            receiver: rx1,
        };

        let t2 = InProcessTransport {
            sender: tx1,
            receiver: rx2,
        };

        (t1, t2)
    }

    /// Create a new transport from existing channel endpoints.
    ///
    /// This allows more advanced setups like broadcast channels or
    /// custom channel configurations.
    pub fn new(
        sender: mpsc::UnboundedSender<Message>,
        receiver: mpsc::UnboundedReceiver<Message>,
    ) -> Self {
        Self { sender, receiver }
    }
}

impl Transport for InProcessTransport {
    type Error = InProcessError;

    async fn send(&mut self, msg: Message) -> Result<(), Self::Error> {
        self.sender
            .send(msg)
            .map_err(|_| InProcessError::ChannelClosed)?;
        Ok(())
    }

    async fn recv(&mut self) -> Result<Message, Self::Error> {
        self.receiver
            .recv()
            .await
            .ok_or(InProcessError::ChannelClosed)
    }

    async fn close(&mut self) -> Result<(), Self::Error> {
        self.receiver.close();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_send_recv() {
        let (mut t1, mut t2) = InProcessTransport::pair();

        let test_data = vec![1, 2, 3, 4, 5];
        let msg = Message::new(test_data.clone());

        t1.send(msg).await.unwrap();
        let received = t2.recv().await.unwrap();

        assert_eq!(received.data, test_data);
    }

    #[tokio::test]
    async fn test_bidirectional() {
        let (mut t1, mut t2) = InProcessTransport::pair();

        // Send from t1 to t2
        let msg1 = Message::new(vec![1, 2, 3]);
        t1.send(msg1.clone()).await.unwrap();
        let received1 = t2.recv().await.unwrap();
        assert_eq!(received1.data, msg1.data);

        // Send from t2 to t1
        let msg2 = Message::new(vec![4, 5, 6]);
        t2.send(msg2.clone()).await.unwrap();
        let received2 = t1.recv().await.unwrap();
        assert_eq!(received2.data, msg2.data);
    }

    #[tokio::test]
    async fn test_multiple_messages() {
        let (mut t1, mut t2) = InProcessTransport::pair();

        for i in 0..100 {
            let data = vec![i; i as usize + 1];
            let msg = Message::new(data.clone());

            t1.send(msg).await.unwrap();
            let received = t2.recv().await.unwrap();

            assert_eq!(received.data, data);
        }
    }

    #[tokio::test]
    async fn test_empty_message() {
        let (mut t1, mut t2) = InProcessTransport::pair();

        let msg = Message::new(vec![]);
        t1.send(msg).await.unwrap();
        let received = t2.recv().await.unwrap();

        assert_eq!(received.data, Vec::<u8>::new());
    }

    #[tokio::test]
    async fn test_large_message() {
        let (mut t1, mut t2) = InProcessTransport::pair();

        let data = vec![42u8; 1_000_000];
        let msg = Message::new(data.clone());

        t1.send(msg).await.unwrap();
        let received = t2.recv().await.unwrap();

        assert_eq!(received.data, data);
    }

    #[tokio::test]
    async fn test_close() {
        let (mut t1, mut t2) = InProcessTransport::pair();

        t2.close().await.unwrap();

        // Sending should fail after close
        let msg = Message::new(vec![1, 2, 3]);
        let result = t1.send(msg).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_concurrent_sends() {
        let (mut t1, mut t2) = InProcessTransport::pair();

        // Spawn task to send multiple messages
        let send_handle = tokio::spawn(async move {
            for i in 0..10 {
                let msg = Message::new(vec![i]);
                t1.send(msg).await.unwrap();
            }
            t1
        });

        // Receive all messages
        let mut received = Vec::new();
        for _ in 0..10 {
            let msg = t2.recv().await.unwrap();
            received.push(msg.data[0]);
        }

        received.sort();
        assert_eq!(received, (0..10).collect::<Vec<_>>());

        send_handle.await.unwrap();
    }
}
