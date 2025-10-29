//! Core traits and types for the RPC framework.
//!
//! This crate provides the fundamental abstractions needed for building
//! transport-agnostic and codec-agnostic RPC systems.

use schema::Schema;
use serde::{Deserialize, Serialize};
use std::fmt;

pub mod error;
pub use error::{Error, Result};

/// Opaque message container for transport layer.
///
/// The message contains raw bytes that will be interpreted by the codec layer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    /// Raw message data
    pub data: Vec<u8>,
}

impl Message {
    /// Create a new message from raw bytes
    pub fn new(data: Vec<u8>) -> Self {
        Self { data }
    }

    /// Create a new message from a byte slice
    pub fn from_slice(data: &[u8]) -> Self {
        Self {
            data: data.to_vec(),
        }
    }
}

/// Transport abstraction for sending and receiving messages.
///
/// This trait is transport-agnostic and can be implemented for any
/// communication mechanism (WebSocket, HTTP, TCP, IPC, etc.).
#[allow(async_fn_in_trait)]
pub trait Transport: Send {
    /// Transport-specific error type
    type Error: std::error::Error + Send + Sync + 'static;

    /// Send a message through the transport
    fn send(&mut self, msg: Message) -> impl std::future::Future<Output = std::result::Result<(), Self::Error>> + Send;

    /// Receive a message from the transport
    fn recv(&mut self) -> impl std::future::Future<Output = std::result::Result<Message, Self::Error>> + Send;

    /// Close the transport gracefully
    fn close(&mut self) -> impl std::future::Future<Output = std::result::Result<(), Self::Error>> + Send {
        async { Ok(()) }
    }
}

/// Codec abstraction for serializing and deserializing data.
///
/// This trait is codec-agnostic and can be implemented for any
/// serialization format (JSON, MessagePack, Protobuf, etc.).
pub trait Codec: Send + Sync {
    /// Encode a value to bytes
    fn encode<T: Serialize + Schema>(&self, value: &T) -> Result<Vec<u8>>;

    /// Decode bytes to a value
    fn decode<T: for<'de> Deserialize<'de> + Schema>(&self, bytes: &[u8]) -> Result<T>;
}

/// RPC request structure
#[derive(Debug, Clone, Serialize, Deserialize, Schema, PartialEq, Eq)]
pub struct RpcRequest {
    /// Request ID for matching responses
    pub id: u64,
    /// Method name to call
    pub method: String,
    /// Serialized method parameters
    pub params: Vec<u8>,
}

/// RPC response structure
#[derive(Debug, Clone, Serialize, Deserialize, Schema)]
pub struct RpcResponse {
    /// Request ID this response corresponds to
    pub id: u64,
    /// Response result
    pub result: ResponseResult,
}

/// Result of an RPC call
#[derive(Debug, Clone, Serialize, Deserialize, Schema)]
pub enum ResponseResult {
    /// Successful result with data
    Ok(Vec<u8>),
    /// Error result with message
    Err(String),
    /// Stream chunk with data (not final)
    StreamChunk(Vec<u8>),
    /// Stream end marker (final chunk)
    StreamEnd,
}

impl fmt::Display for ResponseResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ResponseResult::Ok(_) => write!(f, "Ok"),
            ResponseResult::Err(e) => write!(f, "Err: {}", e),
            ResponseResult::StreamChunk(_) => write!(f, "StreamChunk"),
            ResponseResult::StreamEnd => write!(f, "StreamEnd"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_creation() {
        let data = vec![1, 2, 3, 4];
        let msg = Message::new(data.clone());
        assert_eq!(msg.data, data);
    }

    #[test]
    fn test_message_from_slice() {
        let data = &[1, 2, 3, 4];
        let msg = Message::from_slice(data);
        assert_eq!(msg.data, data);
    }

    #[test]
    fn test_rpc_request_serialization() {
        let request = RpcRequest {
            id: 1,
            method: "test".to_string(),
            params: vec![1, 2, 3],
        };

        let serialized = serde_json::to_vec(&request).unwrap();
        let deserialized: RpcRequest = serde_json::from_slice(&serialized).unwrap();

        assert_eq!(request, deserialized);
    }

    #[test]
    fn test_response_result() {
        let ok_result = ResponseResult::Ok(vec![1, 2, 3]);
        assert_eq!(ok_result.to_string(), "Ok");

        let err_result = ResponseResult::Err("test error".to_string());
        assert_eq!(err_result.to_string(), "Err: test error");

        let chunk_result = ResponseResult::StreamChunk(vec![4, 5, 6]);
        assert_eq!(chunk_result.to_string(), "StreamChunk");

        let end_result = ResponseResult::StreamEnd;
        assert_eq!(end_result.to_string(), "StreamEnd");
    }

    #[test]
    fn test_streaming_response_serialization() {
        let chunk_response = RpcResponse {
            id: 42,
            result: ResponseResult::StreamChunk(vec![1, 2, 3]),
        };

        let serialized = serde_json::to_vec(&chunk_response).unwrap();
        let deserialized: RpcResponse = serde_json::from_slice(&serialized).unwrap();

        assert_eq!(deserialized.id, 42);
        match deserialized.result {
            ResponseResult::StreamChunk(data) => assert_eq!(data, vec![1, 2, 3]),
            _ => panic!("Expected StreamChunk"),
        }

        let end_response = RpcResponse {
            id: 43,
            result: ResponseResult::StreamEnd,
        };

        let serialized = serde_json::to_vec(&end_response).unwrap();
        let deserialized: RpcResponse = serde_json::from_slice(&serialized).unwrap();

        assert_eq!(deserialized.id, 43);
        assert!(matches!(deserialized.result, ResponseResult::StreamEnd));
    }
}
