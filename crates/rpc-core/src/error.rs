//! Error types for the RPC framework.

use thiserror::Error;

/// Result type for RPC operations
pub type Result<T> = std::result::Result<T, Error>;

/// Errors that can occur in RPC operations
#[derive(Error, Debug)]
pub enum Error {
    /// Transport layer error
    #[error("transport error: {0}")]
    Transport(String),

    /// Codec/serialization error
    #[error("codec error: {0}")]
    Codec(String),

    /// Method not found
    #[error("method not found: {0}")]
    MethodNotFound(String),

    /// Invalid request
    #[error("invalid request: {0}")]
    InvalidRequest(String),

    /// Remote error (error from the RPC call itself)
    #[error("remote error: {0}")]
    Remote(String),

    /// Connection closed
    #[error("connection closed")]
    ConnectionClosed,

    /// IO error
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// Generic error
    #[error("{0}")]
    Other(String),
}

impl Error {
    /// Create a transport error
    pub fn transport(msg: impl Into<String>) -> Self {
        Error::Transport(msg.into())
    }

    /// Create a codec error
    pub fn codec(msg: impl Into<String>) -> Self {
        Error::Codec(msg.into())
    }

    /// Create a method not found error
    pub fn method_not_found(method: impl Into<String>) -> Self {
        Error::MethodNotFound(method.into())
    }

    /// Create an invalid request error
    pub fn invalid_request(msg: impl Into<String>) -> Self {
        Error::InvalidRequest(msg.into())
    }

    /// Create a remote error
    pub fn remote(msg: impl Into<String>) -> Self {
        Error::Remote(msg.into())
    }

    /// Create a generic error
    pub fn other(msg: impl Into<String>) -> Self {
        Error::Other(msg.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = Error::transport("connection failed");
        assert_eq!(err.to_string(), "transport error: connection failed");

        let err = Error::codec("invalid json");
        assert_eq!(err.to_string(), "codec error: invalid json");

        let err = Error::method_not_found("unknown_method");
        assert_eq!(err.to_string(), "method not found: unknown_method");
    }

    #[test]
    fn test_error_constructors() {
        let err = Error::transport("test");
        assert!(matches!(err, Error::Transport(_)));

        let err = Error::codec("test");
        assert!(matches!(err, Error::Codec(_)));

        let err = Error::method_not_found("test");
        assert!(matches!(err, Error::MethodNotFound(_)));
    }
}
