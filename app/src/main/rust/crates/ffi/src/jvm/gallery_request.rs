//! V1 complete Gallery request planning with exact-sized output.
use super::{decode_request, deref_direct_bytebuffer, jni_throwing};
use anyhow::{Result, anyhow, ensure};
use eh_net::gallery::request::{GalleryRequest, build_envelope};
use jni::JNIEnv;
use jni::objects::{JByteBuffer, JClass};
use jni::sys::{jbyteArray, jint};
use jni_fn::jni_fn;

/// Returns one bounded request plan; API credentials never enter this adapter.
#[jni_fn("com.hippo.ehviewer.bridge.gallery.GalleryRequestBridgeKt")]
pub fn nativePlanGalleryRequestV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    request_buffer: JByteBuffer<'_>,
    request_length: jint,
) -> jbyteArray {
    jni_throwing(&mut env, |env| {
        let bytes = plan_request_bytes(
            deref_direct_bytebuffer(env, &request_buffer)?,
            request_length,
        )?;
        Ok(env.byte_array_from_slice(&bytes)?.into_raw())
    })
}

fn plan_request_bytes(buffer: &[u8], length: jint) -> Result<Vec<u8>> {
    ensure!(
        length <= 2 * 1024 * 1024,
        "Gallery request exceeds input budget"
    );
    // serde's enum/string errors may quote sensitive request values. Never propagate them.
    let request: GalleryRequest =
        decode_request(buffer, length).map_err(|_| anyhow!("Invalid gallery request CBOR"))?;
    let bytes = serde_cbor::to_vec(&build_envelope(&request))
        .map_err(|_| anyhow!("Invalid gallery request response"))?;
    ensure!(
        bytes.len() <= 16 * 1024 * 1024,
        "Gallery request exceeds output budget"
    );
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(serde::Serialize)]
    struct UncheckedRequest<'a> {
        version: u16,
        site: &'a str,
        kind: &'a str,
    }

    #[test]
    fn malformed_lengths_cbor_and_closed_enums_are_redacted() {
        for (input, length) in [
            (&b""[..], 0),
            (&b"secret-token"[..], -1),
            (&b"secret-token"[..], 13),
            (&b"secret-token"[..], 12),
            (&b"secret-token"[..], 2 * 1024 * 1024 + 1),
        ] {
            let error = plan_request_bytes(input, length).unwrap_err().to_string();
            assert!(!error.contains("secret-token"));
        }
        for (version, site, kind) in [
            (1, "public", "secret-key"),
            (1, "secret-key", "trackerUrl"),
            (256, "public", "torrentUrl"),
        ] {
            let value = UncheckedRequest {
                version,
                site,
                kind,
            };
            let bytes = serde_cbor::to_vec(&value).unwrap();
            let error = plan_request_bytes(&bytes, i32::try_from(bytes.len()).unwrap())
                .unwrap_err()
                .to_string();
            assert_eq!(error, "Invalid gallery request CBOR");
        }
    }

    #[test]
    fn existing_and_navigation_wire_goldens_use_same_boundary() {
        for name in [
            "gallery_request",
            "navigation_favorites",
            "navigation_torrent",
            "navigation_tracker_null",
            "navigation_tracker_empty",
            "navigation_tracker_key",
            "navigation_wiki",
            "navigation_wiki_high",
            "navigation_wiki_low",
            "navigation_wiki_pair_high",
            "navigation_wiki_pair_low",
            "navigation_wiki_delimiters_space",
        ] {
            let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../net/test_data");
            let decode = |suffix| {
                let hex = std::fs::read_to_string(root.join(format!("{name}_{suffix}.cbor.hex")))
                    .unwrap();
                hex.trim()
                    .as_bytes()
                    .chunks_exact(2)
                    .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
                    .collect::<Vec<_>>()
            };
            let input = decode("request");
            assert_eq!(
                plan_request_bytes(&input, i32::try_from(input.len()).unwrap()).unwrap(),
                decode("response")
            );
        }
    }
}
