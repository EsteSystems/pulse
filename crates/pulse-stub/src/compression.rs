use thiserror::Error;

#[derive(Debug, Error)]
pub enum CompressionError {
    #[error("decompression failed: {0}")]
    DecompressFailed(String),
    #[error("compressed output exceeds max size of {max} bytes (got {got})")]
    TooLarge { max: usize, got: usize },
}

/// LZ4 compress UTF-8 content.
pub fn compress(data: &[u8]) -> Vec<u8> {
    lz4_flex::compress_prepend_size(data)
}

/// LZ4 decompress content.
pub fn decompress(data: &[u8]) -> Result<Vec<u8>, CompressionError> {
    lz4_flex::decompress_size_prepended(data)
        .map_err(|e| CompressionError::DecompressFailed(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let original = b"hello pulse, this is a test of LZ4 compression for inline stub content";
        let compressed = compress(original);
        let decompressed = decompress(&compressed).unwrap();
        assert_eq!(&decompressed, original);
    }

    #[test]
    fn empty_roundtrip() {
        let original = b"";
        let compressed = compress(original);
        let decompressed = decompress(&compressed).unwrap();
        assert_eq!(&decompressed, original);
    }

    #[test]
    fn compression_reduces_size_for_repetitive() {
        let original = "pulse ".repeat(100);
        let compressed = compress(original.as_bytes());
        assert!(compressed.len() < original.len());
    }

    #[test]
    fn invalid_data_fails() {
        assert!(decompress(&[0xff, 0xff, 0xff, 0xff]).is_err());
    }
}
