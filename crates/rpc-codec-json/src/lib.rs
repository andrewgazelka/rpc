//! JSON codec for the RPC framework.
//!
//! Provides human-readable JSON serialization using serde_json.
//! Best for web APIs, debugging, and when message inspection is important.

use rpc_core::{Codec, Error, Result};
use schema::Schema;
use serde::{Deserialize, Serialize};

/// JSON codec using serde_json.
///
/// # Examples
///
/// ```
/// use rpc_core::Codec;
/// use rpc_codec_json::JsonCodec;
///
/// let codec = JsonCodec;
/// let data = vec![1, 2, 3, 4, 5];
///
/// let encoded = codec.encode(&data).unwrap();
/// let decoded: Vec<i32> = codec.decode(&encoded).unwrap();
///
/// assert_eq!(data, decoded);
/// ```
#[derive(Debug, Clone, Copy, Default)]
pub struct JsonCodec;

impl Codec for JsonCodec {
    fn encode<T: Serialize + Schema>(&self, value: &T) -> Result<Vec<u8>> {
        serde_json::to_vec(value).map_err(|e| Error::codec(format!("JSON encode error: {}", e)))
    }

    fn decode<T: for<'de> Deserialize<'de> + Schema>(&self, bytes: &[u8]) -> Result<T> {
        serde_json::from_slice(bytes).map_err(|e| Error::codec(format!("JSON decode error: {}", e)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_json_encode_decode() {
        let codec = JsonCodec;
        let value = vec![1, 2, 3, 4, 5];

        let encoded = codec.encode(&value).unwrap();
        let decoded: Vec<i32> = codec.decode(&encoded).unwrap();

        assert_eq!(value, decoded);
    }

    #[test]
    fn test_json_human_readable() {
        let codec = JsonCodec;
        let value = ("hello".to_string(), 42);

        let encoded = codec.encode(&value).unwrap();
        let json_str = String::from_utf8(encoded).unwrap();

        // JSON should be human-readable
        assert!(json_str.contains("hello"));
        assert!(json_str.contains("42"));
    }

    #[test]
    fn test_json_complex_types() {
        let codec = JsonCodec;
        let value = (42, "test".to_string(), true);

        let encoded = codec.encode(&value).unwrap();
        let decoded: (i32, String, bool) = codec.decode(&encoded).unwrap();

        assert_eq!(value, decoded);
    }

    #[test]
    fn test_json_error_handling() {
        let codec = JsonCodec;
        let invalid = b"not valid json";

        let result: Result<Vec<i32>> = codec.decode(invalid);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::Codec(_)));
    }

    #[test]
    fn test_json_nested_structures() {
        #[derive(Debug, Serialize, Deserialize, Schema, PartialEq)]
        struct Inner {
            value: i32,
        }

        #[derive(Debug, Serialize, Deserialize, Schema, PartialEq)]
        struct Outer {
            name: String,
            inner: Inner,
        }

        let codec = JsonCodec;
        let data = Outer {
            name: "test".to_string(),
            inner: Inner { value: 42 },
        };

        let encoded = codec.encode(&data).unwrap();
        let decoded: Outer = codec.decode(&encoded).unwrap();

        assert_eq!(data, decoded);
    }
}
