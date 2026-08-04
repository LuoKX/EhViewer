//! Checked CBOR and input-slice operations shared by boundary adapters.

use std::io::Cursor;
use std::str::from_utf8;

use anyhow::{Context, Result, ensure};
use serde::{Serialize, de::DeserializeOwned};

pub(crate) fn input_bytes(buffer: &[u8], limit: i32) -> Result<&[u8]> {
    let input_len = usize::try_from(limit).context("Negative input length")?;
    ensure!(input_len > 0, "Empty input");
    ensure!(
        input_len <= buffer.len(),
        "Input length {input_len} exceeds direct buffer capacity {}",
        buffer.len()
    );
    Ok(&buffer[..input_len])
}

pub(crate) fn input_text(buffer: &[u8], limit: i32) -> Result<&str> {
    from_utf8(input_bytes(buffer, limit)?).context("Response is not valid UTF-8")
}

pub(crate) fn decode<T: DeserializeOwned>(buffer: &[u8], limit: i32) -> Result<T> {
    serde_cbor::from_slice(input_bytes(buffer, limit)?).context("Input is not valid CBOR")
}

pub(crate) fn encode_into<T: Serialize>(buffer: &mut [u8], value: &T) -> Result<i32> {
    let mut encoded = Vec::new();
    serde_cbor::to_writer(Cursor::new(&mut encoded), value)?;
    let encoded_len = i32::try_from(encoded.len()).context("Encoded CBOR response is too large")?;
    ensure!(
        encoded.len() <= buffer.len(),
        "Encoded CBOR response requires {} bytes but the direct buffer holds {}",
        encoded.len(),
        buffer.len()
    );
    buffer[..encoded.len()].copy_from_slice(&encoded);
    Ok(encoded_len)
}

#[cfg(test)]
mod tests {
    use serde::{Deserialize, Serialize};

    use super::{decode, encode_into, input_bytes, input_text};

    #[derive(Debug, Deserialize, Serialize, PartialEq)]
    struct Message {
        value: String,
    }

    #[test]
    fn text_input_validates_length_and_utf8() {
        assert_eq!(input_text(b"body", 4).expect("valid body"), "body");
        assert!(
            input_text(b"body", 0)
                .unwrap_err()
                .to_string()
                .contains("Empty")
        );
        assert!(
            input_text(b"body", -1)
                .unwrap_err()
                .to_string()
                .contains("Negative")
        );
        assert!(
            input_text(b"body", 5)
                .unwrap_err()
                .to_string()
                .contains("capacity")
        );
        assert!(
            input_text(&[0xff], 1)
                .unwrap_err()
                .to_string()
                .contains("UTF-8")
        );
    }

    #[test]
    fn byte_and_cbor_inputs_validate_length_before_decoding() {
        assert_eq!(input_bytes(b"body", 4).expect("valid bytes"), b"body");
        assert!(input_bytes(b"body", 0).is_err());
        assert!(input_bytes(b"body", -1).is_err());
        assert!(input_bytes(b"body", 5).is_err());

        let message = Message {
            value: "request".to_owned(),
        };
        let encoded = serde_cbor::to_vec(&message).expect("request should encode");
        let encoded_len = i32::try_from(encoded.len()).expect("test request length should fit");
        assert_eq!(
            decode::<Message>(&encoded, encoded_len).expect("request should decode"),
            message
        );
        assert!(decode::<Message>(&[0xff], 1).is_err());
    }

    #[test]
    fn encoding_is_atomic_when_the_output_buffer_is_too_small() {
        let value = Message {
            value: "payload".to_owned(),
        };
        let mut full = [0_u8; 64];
        let length = encode_into(&mut full, &value).expect("message should serialize");
        let length = usize::try_from(length).expect("encoded length should be non-negative");
        let decoded: Message =
            serde_cbor::from_slice(&full[..length]).expect("serialized message should decode");
        assert_eq!(decoded, value);

        let mut small = [0x7f_u8; 2];
        let before = small;
        assert!(encode_into(&mut small, &value).is_err());
        assert_eq!(small, before);
    }
}
