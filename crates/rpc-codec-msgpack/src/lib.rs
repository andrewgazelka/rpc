//! MessagePack codec for the RPC framework.
//!
//! Provides compact binary serialization using rmp-serde.
//! Best for high-throughput applications and bandwidth-constrained scenarios.

use rpc_core::{Codec, Error, Result};
use serde::{Deserialize, Serialize};

/// MessagePack codec using rmp-serde.
///
/// MessagePack provides compact binary serialization with better performance
/// and smaller message sizes compared to JSON.
///
/// # Examples
///
/// ```
/// use rpc_core::Codec;
/// use rpc_codec_msgpack::MessagePackCodec;
///
/// let codec = MessagePackCodec;
/// let data = vec![1, 2, 3, 4, 5];
///
/// let encoded = codec.encode(&data).unwrap();
/// let decoded: Vec<i32> = codec.decode(&encoded).unwrap();
///
/// assert_eq!(data, decoded);
/// ```
#[derive(Debug, Clone, Copy, Default)]
pub struct MessagePackCodec;

impl Codec for MessagePackCodec {
    fn encode<T: Serialize>(&self, value: &T) -> Result<Vec<u8>> {
        rmp_serde::to_vec(value)
            .map_err(|e| Error::codec(format!("MessagePack encode error: {}", e)))
    }

    fn decode<T: for<'de> Deserialize<'de>>(&self, bytes: &[u8]) -> Result<T> {
        rmp_serde::from_slice(bytes)
            .map_err(|e| Error::codec(format!("MessagePack decode error: {}", e)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_msgpack_encode_decode() {
        let codec = MessagePackCodec;
        let value = vec![1, 2, 3, 4, 5];

        let encoded = codec.encode(&value).unwrap();
        let decoded: Vec<i32> = codec.decode(&encoded).unwrap();

        assert_eq!(value, decoded);
    }

    #[test]
    fn test_msgpack_compact() {
        let codec = MessagePackCodec;
        let value = vec![1, 2, 3, 4, 5];

        let encoded = codec.encode(&value).unwrap();

        // MessagePack should be compact
        // For a small array like this, should be less than 10 bytes
        assert!(encoded.len() < 10);
    }

    #[test]
    fn test_msgpack_complex_types() {
        use std::collections::HashMap;

        let codec = MessagePackCodec;
        let mut map = HashMap::new();
        map.insert("key1".to_string(), 42);
        map.insert("key2".to_string(), 100);

        let encoded = codec.encode(&map).unwrap();
        let decoded: HashMap<String, i32> = codec.decode(&encoded).unwrap();

        assert_eq!(map, decoded);
    }

    #[test]
    fn test_msgpack_error_handling() {
        let codec = MessagePackCodec;
        let invalid = b"\xFF\xFF\xFF";

        let result: Result<Vec<i32>> = codec.decode(invalid);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), Error::Codec(_)));
    }

    #[test]
    fn test_msgpack_nested_structures() {
        #[derive(Debug, Serialize, Deserialize, PartialEq)]
        struct Inner {
            value: i32,
        }

        #[derive(Debug, Serialize, Deserialize, PartialEq)]
        struct Outer {
            name: String,
            inner: Inner,
        }

        let codec = MessagePackCodec;
        let data = Outer {
            name: "test".to_string(),
            inner: Inner { value: 42 },
        };

        let encoded = codec.encode(&data).unwrap();
        let decoded: Outer = codec.decode(&encoded).unwrap();

        assert_eq!(data, decoded);
    }

    #[test]
    fn test_msgpack_size_comparison() {
        // Compare MessagePack size to demonstrate compactness
        let codec = MessagePackCodec;
        let large_data: Vec<i32> = (0..1000).collect();

        let encoded = codec.encode(&large_data).unwrap();

        // MessagePack encoding should be reasonably compact
        // 1000 integers should take less than 5KB
        assert!(encoded.len() < 5000);
    }
}
