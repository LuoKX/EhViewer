//! Compatibility JNI hash exports owned by Rust.

use std::io::Read;

use anyhow::Context as _;
use jni::JNIEnv;
use jni::objects::JClass;
use jni::sys::{jint, jstring};
use jni_fn::jni_fn;
use sha1::{Digest as _, Sha1};

use super::jni_throwing;

/// Computes the lowercase SHA-1 digest from a Java-owned descriptor's current position.
///
/// The descriptor is duplicated before reading, so Rust closes only its own handle.
/// POSIX duplicates share the open-file offset, matching the previous C implementation.
#[jni_fn("com.hippo.ehviewer.jni.HashKt")]
pub fn sha1(mut env: JNIEnv<'_>, _: JClass<'_>, file_descriptor: jint) -> jstring {
    jni_throwing(&mut env, |env| {
        let file = super::duplicate_file_descriptor(file_descriptor, "SHA-1")
            .context("Failed to duplicate SHA-1 input descriptor")?;
        let value = digest_reader(file).context("Failed to read SHA-1 input descriptor")?;
        Ok(env.new_string(value)?.into_raw())
    })
}

fn digest_reader(mut reader: impl Read) -> std::io::Result<String> {
    let mut digest = Sha1::new();
    let mut buffer = [0_u8; 8192];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, Error, Read};

    use super::digest_reader;

    #[test]
    fn hashes_empty_and_multi_buffer_inputs_as_lowercase_sha1() {
        assert_eq!(
            digest_reader(Cursor::new(Vec::<u8>::new())).unwrap(),
            "da39a3ee5e6b4b0d3255bfef95601890afd80709"
        );
        assert_eq!(
            digest_reader(Cursor::new(vec![b'a'; 16_385])).unwrap(),
            "521b836124c4df1973c7be366d60dc7230b403b7"
        );
    }

    #[test]
    fn surfaces_descriptor_read_failures() {
        struct FailingReader;
        impl Read for FailingReader {
            fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
                Err(Error::other("fixed failure"))
            }
        }

        assert!(digest_reader(FailingReader).is_err());
    }
}
