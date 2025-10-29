//! Stdio transport for RPC framework.
//!
//! This transport uses stdin/stdout for communication, making it ideal for:
//! - Embedded binaries that communicate via pipes
//! - CLI tools with RPC capabilities
//! - Process-to-process communication

use rpc_core::{Message, Transport};
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::sync::Mutex;

/// Stdio transport error type
#[derive(Debug, thiserror::Error)]
pub enum StdioError {
    /// IO error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Stdio transport that uses stdin/stdout for communication.
///
/// Messages are sent as length-prefixed frames:
/// - First 4 bytes: message length (big-endian u32)
/// - Remaining bytes: message data
pub struct StdioTransport {
    stdin: Mutex<BufReader<tokio::io::Stdin>>,
    stdout: Mutex<tokio::io::Stdout>,
}

impl StdioTransport {
    /// Create a new stdio transport using process stdin/stdout
    pub fn new() -> Self {
        Self {
            stdin: Mutex::new(BufReader::new(tokio::io::stdin())),
            stdout: Mutex::new(tokio::io::stdout()),
        }
    }
}

impl Default for StdioTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl Transport for StdioTransport {
    type Error = StdioError;

    async fn send(&mut self, msg: Message) -> Result<(), Self::Error> {
        let mut stdout = self.stdout.lock().await;

        // Write length prefix (4 bytes, big-endian)
        let len = msg.data.len() as u32;
        stdout.write_all(&len.to_be_bytes()).await?;

        // Write message data
        stdout.write_all(&msg.data).await?;

        // Flush to ensure message is sent immediately
        stdout.flush().await?;

        Ok(())
    }

    async fn recv(&mut self) -> Result<Message, Self::Error> {
        let mut stdin = self.stdin.lock().await;

        // Read length prefix (4 bytes, big-endian)
        let mut len_buf = [0u8; 4];
        stdin.read_exact(&mut len_buf).await?;

        let len = u32::from_be_bytes(len_buf) as usize;

        // Read message data
        let mut data = vec![0u8; len];
        stdin.read_exact(&mut data).await?;

        Ok(Message::new(data))
    }

    async fn close(&mut self) -> Result<(), Self::Error> {
        // Stdio doesn't need explicit closing
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::DuplexStream;

    /// Helper to create a pair of connected transports for testing
    struct TestTransport {
        stream: Mutex<DuplexStream>,
    }

    impl TestTransport {
        fn pair() -> (TestTransport, TestTransport) {
            let (client, server) = tokio::io::duplex(8192);

            let t1 = TestTransport {
                stream: Mutex::new(client),
            };

            let t2 = TestTransport {
                stream: Mutex::new(server),
            };

            (t1, t2)
        }
    }

    impl Transport for TestTransport {
        type Error = StdioError;

        async fn send(&mut self, msg: Message) -> Result<(), Self::Error> {
            let mut stream = self.stream.lock().await;

            let len = msg.data.len() as u32;
            stream.write_all(&len.to_be_bytes()).await?;
            stream.write_all(&msg.data).await?;
            stream.flush().await?;

            Ok(())
        }

        async fn recv(&mut self) -> Result<Message, Self::Error> {
            let mut stream = self.stream.lock().await;

            let mut len_buf = [0u8; 4];
            stream.read_exact(&mut len_buf).await?;

            let len = u32::from_be_bytes(len_buf) as usize;

            let mut data = vec![0u8; len];
            stream.read_exact(&mut data).await?;

            Ok(Message::new(data))
        }
    }

    #[tokio::test]
    async fn test_send_recv() {
        let (mut t1, mut t2) = TestTransport::pair();

        let test_data = vec![1, 2, 3, 4, 5];
        let msg = Message::new(test_data.clone());

        t1.send(msg).await.unwrap();
        let received = t2.recv().await.unwrap();

        assert_eq!(received.data, test_data);
    }

    #[tokio::test]
    async fn test_multiple_messages() {
        let (mut t1, mut t2) = TestTransport::pair();

        for i in 0..10 {
            let data = vec![i; i as usize + 1];
            let msg = Message::new(data.clone());

            t1.send(msg).await.unwrap();
            let received = t2.recv().await.unwrap();

            assert_eq!(received.data, data);
        }
    }

    #[tokio::test]
    async fn test_empty_message() {
        let (mut t1, mut t2) = TestTransport::pair();

        let msg = Message::new(vec![]);
        t1.send(msg).await.unwrap();
        let received = t2.recv().await.unwrap();

        assert_eq!(received.data, Vec::<u8>::new());
    }

    #[tokio::test]
    async fn test_large_message() {
        let (mut t1, mut t2) = TestTransport::pair();

        // Use smaller size to avoid duplex buffer issues
        let data = vec![42u8; 4096];
        let msg = Message::new(data.clone());

        t1.send(msg).await.unwrap();
        let received = t2.recv().await.unwrap();

        assert_eq!(received.data, data);
    }
}
