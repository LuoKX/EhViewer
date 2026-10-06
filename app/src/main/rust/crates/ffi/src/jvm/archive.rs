//! Android descriptor adapters for generated and external ZIP/CBZ archives.

use std::fs::File;
use std::io::{Seek, SeekFrom};

use anyhow::{Context as _, Result, ensure};
use eh_archive::writer::{CbzEntry, write_generated_cbz};
use eh_archive::{MAX_GENERATED_CBZ_IMAGES, validate_generated_cbz};
use jni::JNIEnv;
#[cfg(target_os = "android")]
use jni::objects::{JByteArray, JByteBuffer};
use jni::objects::{JClass, JIntArray, JObjectArray, JString};
#[cfg(target_os = "android")]
use jni::sys::{JNI_TRUE, jboolean, jbyteArray, jstring};
use jni::sys::{jint, jlong};
use jni_fn::jni_fn;

use super::jni_throwing;
#[cfg(target_os = "android")]
use super::{decode_request, deref_direct_bytebuffer, deref_mut_direct_bytebuffer};

#[cfg(any(target_os = "android", all(test, feature = "jvm")))]
mod archive_native;

#[cfg(target_os = "android")]
use archive_native::{ARCHIVE_READER_REGISTRY, RecoveryMetadataKind};

/// Writes one application-generated CBZ through `eh_archive` V1.
///
/// Source and output descriptors are duplicated, so Rust closes only its own
/// handles. Like the legacy writer, each fresh source descriptor is consumed
/// from its current position to EOF and output starts at the caller's current
/// position. Invalid inputs and write failures become contained Java
/// `RuntimeException`s.
#[jni_fn("com.hippo.ehviewer.bridge.archive.ArchiveWriterBridgeKt")]
pub fn nativeWriteGeneratedCbzV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    source_descriptors: JIntArray<'_>,
    entry_names: JObjectArray<'_>,
    output_descriptor: jint,
    expected_images: jint,
) -> jlong {
    jni_throwing(&mut env, |env| {
        let expected_images = usize::try_from(expected_images)
            .context("Generated CBZ expected image count is negative")?;
        ensure!(
            (1..=MAX_GENERATED_CBZ_IMAGES).contains(&expected_images),
            "Generated CBZ expected image count is outside the supported range"
        );
        let expected_entries = expected_images
            .checked_add(1)
            .context("Generated CBZ expected entry count overflow")?;
        let descriptor_count = usize::try_from(env.get_array_length(&source_descriptors)?)
            .context("Generated CBZ descriptor count exceeds Rust range")?;
        let name_count = usize::try_from(env.get_array_length(&entry_names)?)
            .context("Generated CBZ name count exceeds Rust range")?;
        ensure!(
            descriptor_count == expected_entries && name_count == expected_entries,
            "Generated CBZ descriptor and name counts must match pages plus metadata"
        );

        let mut descriptors = vec![0; descriptor_count];
        env.get_int_array_region(&source_descriptors, 0, &mut descriptors)?;
        let mut entries = Vec::with_capacity(descriptor_count);
        for (index, descriptor) in descriptors.into_iter().enumerate() {
            let name = env.with_local_frame(2, |env| -> Result<String> {
                let object = env.get_object_array_element(&entry_names, i32::try_from(index)?)?;
                ensure!(!object.is_null(), "Generated CBZ entry name is null");
                let string = JString::from(object);
                Ok(env.get_string(&string)?.into())
            })?;
            let source = duplicate_file_descriptor(descriptor)
                .with_context(|| format!("Failed to duplicate CBZ source descriptor {index}"))?;
            entries.push(CbzEntry::new(name, source));
        }

        let mut output_file = duplicate_file_descriptor(output_descriptor)
            .context("Failed to duplicate generated CBZ output descriptor")?;
        let write_result = write_generated_cbz(&mut output_file, entries, expected_images)
            .map(|(_, summary)| summary)
            .context("Generated CBZ write failed");
        if write_result.is_err() {
            reset_failed_archive(&mut output_file);
        }
        let summary = write_result?;
        i64::try_from(summary.uncompressed_bytes)
            .context("Generated CBZ byte count exceeds JNI range")
    })
}

fn reset_failed_archive(output_file: &mut File) {
    if output_file.set_len(0).is_ok() {
        output_file.seek(SeekFrom::Start(0)).ok();
    }
}

fn duplicate_file_descriptor(file_descriptor: jint) -> Result<File> {
    super::duplicate_file_descriptor(file_descriptor, "Archive")
}

/// Opens one ZIP, 7z, RAR, or tar archive through a Rust-owned V2 handle.
#[cfg(target_os = "android")]
#[jni_fn("com.hippo.ehviewer.bridge.archive.ArchiveReaderBridgeKt")]
pub fn nativeOpenArchiveV2(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    file_descriptor: jint,
    sort_entries: jboolean,
) -> jlong {
    jni_throwing(&mut env, |_| {
        let archive_file = duplicate_file_descriptor(file_descriptor)?;
        ARCHIVE_READER_REGISTRY
            .open(archive_file, sort_entries != 0)
            .context("Failed to open Rust multi-format archive reader")
    })
}

/// Returns whether an opened V2 archive requires a passphrase before extraction.
#[cfg(target_os = "android")]
#[jni_fn("com.hippo.ehviewer.bridge.archive.ArchiveReaderBridgeKt")]
pub fn nativeArchiveNeedsPasswordV2(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    reader_handle: jlong,
) -> jboolean {
    jni_throwing(&mut env, |_| {
        Ok(if ARCHIVE_READER_REGISTRY.needs_password(reader_handle)? {
            JNI_TRUE
        } else {
            0
        })
    })
}

/// Validates and installs one passphrase without changing state on rejection.
#[cfg(target_os = "android")]
#[jni_fn("com.hippo.ehviewer.bridge.archive.ArchiveReaderBridgeKt")]
pub fn nativeProvideArchivePasswordV2(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    reader_handle: jlong,
    password: JByteArray<'_>,
) -> jboolean {
    jni_throwing(&mut env, |env| {
        let password = env.convert_byte_array(&password)?;
        Ok(
            if ARCHIVE_READER_REGISTRY.provide_password(reader_handle, password)? {
                JNI_TRUE
            } else {
                0
            },
        )
    })
}

/// Returns the playable image count after any required password was accepted.
#[cfg(target_os = "android")]
#[jni_fn("com.hippo.ehviewer.bridge.archive.ArchiveReaderBridgeKt")]
pub fn nativeArchiveEntryCountV2(mut env: JNIEnv<'_>, _: JClass<'_>, reader_handle: jlong) -> jint {
    jni_throwing(&mut env, |_| {
        let count = ARCHIVE_READER_REGISTRY.entry_count(reader_handle)?;
        i32::try_from(count).context("Rust archive entry count exceeds JNI range")
    })
}

/// Returns one V2 image entry's uncompressed size.
#[cfg(target_os = "android")]
#[jni_fn("com.hippo.ehviewer.bridge.archive.ArchiveReaderBridgeKt")]
pub fn nativeArchiveEntrySizeV2(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    reader_handle: jlong,
    index: jint,
) -> jlong {
    jni_throwing(&mut env, |_| {
        let size =
            ARCHIVE_READER_REGISTRY.entry_size(reader_handle, archive_entry_index(index)?)?;
        i64::try_from(size).context("Rust archive entry size exceeds JNI range")
    })
}

/// Returns one V2 image entry's filtered lowercase extension.
#[cfg(target_os = "android")]
#[jni_fn("com.hippo.ehviewer.bridge.archive.ArchiveReaderBridgeKt")]
pub fn nativeArchiveEntryExtensionV2(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    reader_handle: jlong,
    index: jint,
) -> jstring {
    jni_throwing(&mut env, |env| {
        let extension =
            ARCHIVE_READER_REGISTRY.extension(reader_handle, archive_entry_index(index)?)?;
        Ok(env.new_string(extension)?.into_raw())
    })
}

/// Returns one V2 image entry's original archive pathname.
#[cfg(target_os = "android")]
#[jni_fn("com.hippo.ehviewer.bridge.archive.ArchiveReaderBridgeKt")]
pub fn nativeArchiveEntryNameV2(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    reader_handle: jlong,
    index: jint,
) -> jstring {
    jni_throwing(&mut env, |env| {
        let name =
            ARCHIVE_READER_REGISTRY.entry_name(reader_handle, archive_entry_index(index)?)?;
        Ok(env.new_string(name)?.into_raw())
    })
}

/// Decodes one V2 image to a Rust sink so recovery can prove it is readable.
#[cfg(target_os = "android")]
#[jni_fn("com.hippo.ehviewer.bridge.archive.ArchiveReaderBridgeKt")]
pub fn nativeVerifyArchiveEntryV2(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    reader_handle: jlong,
    index: jint,
) -> jboolean {
    jni_throwing(&mut env, |_| {
        ARCHIVE_READER_REGISTRY.verify_entry(reader_handle, archive_entry_index(index)?)?;
        Ok(JNI_TRUE)
    })
}

/// Returns one bounded V3 recovery batch from the same V2 opaque handle.
///
/// The codec retains the expected cursor/readability state, validates the request
/// before decoding, and contains failures without exporting filenames in errors.
#[cfg(target_os = "android")]
#[jni_fn("com.hippo.ehviewer.bridge.archive.ArchiveReaderBridgeKt")]
pub fn nativeInspectArchiveRecoveryV3(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    reader_handle: jlong,
    request_buffer: JByteBuffer<'_>,
    request_size: jint,
) -> jbyteArray {
    jni_throwing(&mut env, |env| {
        ensure!(
            request_size <= 256,
            "Archive recovery request exceeds wire limit"
        );
        let input = deref_direct_bytebuffer(env, &request_buffer)?;
        let request = decode_request(input, request_size)?;
        let batch = ARCHIVE_READER_REGISTRY.inspect_recovery_batch(reader_handle, &request)?;
        let envelope = eh_domain::contracts::ApiEnvelope::ok(3, "archiveRecovery", batch);
        let bytes = serde_cbor::to_vec(&envelope)?;
        ensure!(
            bytes.len() <= 3 * 1024 * 1024,
            "Archive recovery response exceeds wire limit"
        );
        Ok(env.byte_array_from_slice(&bytes)?.into_raw())
    })
}

/// Returns whether any archive pathname is absolute or contains parent traversal.
#[cfg(target_os = "android")]
#[jni_fn("com.hippo.ehviewer.bridge.archive.ArchiveReaderBridgeKt")]
pub fn nativeArchiveHasUnsafePathV2(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    reader_handle: jlong,
) -> jboolean {
    jni_throwing(&mut env, |_| {
        Ok(if ARCHIVE_READER_REGISTRY.has_unsafe_path(reader_handle)? {
            JNI_TRUE
        } else {
            0
        })
    })
}

/// Extracts one bounded recovery sidecar, or returns null when it is absent.
#[cfg(target_os = "android")]
#[jni_fn("com.hippo.ehviewer.bridge.archive.ArchiveReaderBridgeKt")]
pub fn nativeArchiveRecoveryMetadataV2(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    reader_handle: jlong,
    kind: jint,
) -> jbyteArray {
    jni_throwing(&mut env, |env| {
        let kind = match kind {
            0 => RecoveryMetadataKind::SpiderInfo,
            1 => RecoveryMetadataKind::ComicInfo,
            _ => anyhow::bail!("Unknown recovery metadata kind"),
        };
        match ARCHIVE_READER_REGISTRY.recovery_metadata(reader_handle, kind)? {
            Some(bytes) => Ok(env.byte_array_from_slice(&bytes)?.into_raw()),
            None => Ok(std::ptr::null_mut()),
        }
    })
}

/// Decodes one V2 image into a Java-owned direct buffer.
#[cfg(target_os = "android")]
#[jni_fn("com.hippo.ehviewer.bridge.archive.ArchiveReaderBridgeKt")]
pub fn nativeExtractArchiveEntryV2(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    reader_handle: jlong,
    index: jint,
    output_buffer: JByteBuffer<'_>,
) -> jint {
    jni_throwing(&mut env, |env| {
        let output = deref_mut_direct_bytebuffer(env, &output_buffer)?;
        let output_capacity = output.len();
        let copied = {
            let mut writer = &mut output[..];
            ARCHIVE_READER_REGISTRY.extract_to(
                reader_handle,
                archive_entry_index(index)?,
                &mut writer,
            )?
        };
        ensure!(
            copied == u64::try_from(output_capacity)?,
            "Rust archive output buffer capacity does not match entry size"
        );
        i32::try_from(copied).context("Rust archive extracted byte count exceeds JNI range")
    })
}

/// Streams one V2 image into an Android-owned output descriptor.
#[cfg(target_os = "android")]
#[jni_fn("com.hippo.ehviewer.bridge.archive.ArchiveReaderBridgeKt")]
pub fn nativeExtractArchiveEntryToFdV2(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    reader_handle: jlong,
    index: jint,
    file_descriptor: jint,
) -> jboolean {
    jni_throwing(&mut env, |_| {
        let mut output_file = duplicate_file_descriptor(file_descriptor)?;
        ARCHIVE_READER_REGISTRY.extract_to(
            reader_handle,
            archive_entry_index(index)?,
            &mut output_file,
        )?;
        Ok(JNI_TRUE)
    })
}

/// Idempotently closes one V2 Rust archive reader handle.
#[cfg(target_os = "android")]
#[jni_fn("com.hippo.ehviewer.bridge.archive.ArchiveReaderBridgeKt")]
pub fn nativeCloseArchiveV2(mut env: JNIEnv<'_>, _: JClass<'_>, reader_handle: jlong) -> jboolean {
    jni_throwing(&mut env, |_| {
        Ok(if ARCHIVE_READER_REGISTRY.close(reader_handle)? {
            JNI_TRUE
        } else {
            0
        })
    })
}

#[cfg(target_os = "android")]
fn archive_entry_index(index: jint) -> Result<usize> {
    usize::try_from(index).context("Rust archive image index is negative")
}

/// Validates one completed application-generated CBZ through `eh_archive` V1.
///
/// The Java-owned descriptor remains open and its shared seek position is
/// restored before this synchronous JNI call returns. Validation failures and
/// invalid descriptors become contained Java `RuntimeException`s.
#[jni_fn("com.hippo.ehviewer.bridge.archive.ArchiveValidationBridgeKt")]
pub fn nativeValidateDownloadedCbzV1(
    mut env: JNIEnv<'_>,
    _: JClass<'_>,
    file_descriptor: jint,
    expected_images: jint,
) -> jlong {
    jni_throwing(&mut env, |_| {
        let expected_images = usize::try_from(expected_images)
            .context("Downloaded CBZ expected image count is negative")?;
        let mut archive_file = duplicate_file_descriptor(file_descriptor)?;
        let original_position = archive_file
            .stream_position()
            .context("Failed to read downloaded CBZ descriptor position")?;

        let validation = archive_file
            .seek(SeekFrom::Start(0))
            .context("Failed to seek to downloaded CBZ start")
            .and_then(|_| {
                validate_generated_cbz(&mut archive_file, expected_images)
                    .context("Downloaded CBZ validation failed")
            });
        archive_file
            .seek(SeekFrom::Start(original_position))
            .context("Failed to restore downloaded CBZ descriptor position")?;

        let validation = validation?;
        i64::try_from(validation.uncompressed_bytes)
            .context("Downloaded CBZ validated byte count exceeds JNI range")
    })
}
