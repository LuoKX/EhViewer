use super::*;
use sha2::{Digest as _, Sha256};

#[test]
fn pinned_c_animation_golden_pixels_timing_blend_dispose_and_reset() {
    for (encoded, expected) in [
        (
            include_bytes!("test_data/lossless-blend.webp").as_slice(),
            include_bytes!("test_data/lossless-blend.rgba").as_slice(),
        ),
        (
            include_bytes!("test_data/lossy-alpha.webp").as_slice(),
            include_bytes!("test_data/lossy-alpha.rgba").as_slice(),
        ),
    ] {
        let mut decoder = AnimationDecoder::new(encoded).unwrap();
        assert_eq!(
            decoder.info(),
            ImageInfo {
                width: 6,
                height: 4,
                loop_count: 3,
                frame_count: 6
            }
        );
        for (index, t) in [13, 50, 50, 79, 86, 105].into_iter().enumerate() {
            decoder
                .next_frame(false, |frame, _, time| {
                    assert_eq!(time, t);
                    let reference = &expected[index * 96..(index + 1) * 96];
                    assert_eq!(frame, reference, "frame {index}");
                    Ok(())
                })
                .unwrap();
        }
        decoder
            .next_frame(true, |frame, _, t| {
                assert_eq!(t, 13);
                assert_eq!(frame, &expected[..96]);
                Ok(())
            })
            .unwrap();
    }
}
#[test]
fn existing_animation_matches_pinned_c_frame_hashes() {
    let mut decoder = AnimationDecoder::new(&hex(include_str!(
        "../../../../test_data/animated.webp.hex"
    )))
    .unwrap();
    for hash in include_str!("test_data/animated.sha256").lines() {
        decoder
            .next_frame(false, |frame, _, _| {
                assert_eq!(Sha256::digest(frame).as_slice(), hex(hash).as_slice());
                Ok(())
            })
            .unwrap();
    }
}

fn chunk(tag: [u8; 4], body: &[u8]) -> Vec<u8> {
    let mut result = tag.to_vec();
    result.extend(u32::try_from(body.len()).unwrap().to_le_bytes());
    result.extend(body);
    result.resize(result.len() + body.len() % 2, 0);
    result
}
fn riff(chunks: &[u8]) -> Vec<u8> {
    let mut out = b"RIFF".to_vec();
    out.extend(u32::try_from(chunks.len() + 4).unwrap().to_le_bytes());
    out.extend(b"WEBP");
    out.extend(chunks);
    out
}

#[test]
fn static_metadata_in_place_rgb_expansion_and_legacy_zero_timestamp() {
    for bytes in [
        include_bytes!("test_data/lossless-blend.webp").as_slice(),
        include_bytes!("test_data/lossy-alpha.webp").as_slice(),
    ] {
        let original = AnimationDecoder::new(bytes).unwrap();
        let frame = &original.frames[0];
        let mut source = FrameSource::new(
            &original.data[frame.payload.clone()],
            frame.width,
            frame.height,
        )
        .unwrap();
        assert_eq!(
            source.seek(SeekFrom::End(-1)).unwrap() + 1,
            30 + frame.payload.len() as u64
        );
        assert!(source.seek(SeekFrom::End(1)).is_err());
        assert!(source.seek(SeekFrom::Current(i64::MIN)).is_err());
        source.seek(SeekFrom::Start(0)).unwrap();
        let mut static_image = Vec::new();
        source.read_to_end(&mut static_image).unwrap();
        let mut decoder = AnimationDecoder::new(&static_image).unwrap();
        assert_eq!(decoder.info().frame_count, 1);
        assert_eq!(decoder.info().loop_count, 1);
        assert!(decoder.next_frame(false, |_, _, _| Ok(())).is_err());
        let raw = riff(&original.data[frame.payload.clone()]);
        assert!(AnimationDecoder::new(&raw).is_ok());
    }
    let unknown = chunk(*b"JUNK", b"ignored");
    assert!(payload_dimensions(&unknown, 0..unknown.len()).is_err());
    assert!(AnimationDecoder::new(&riff(&unknown)).is_err());
}

#[test]
fn frame_metadata_chunk_order_and_resource_errors_are_explicit() {
    for data in [
        riff(&chunk(*b"VP8X", &[0; 9])),
        riff(&chunk(*b"ANIM", &[0; 6])),
        riff(&chunk(*b"ANMF", &[0; 16])),
        riff(b"1234"),
        riff(&[b'J', b'U', b'N', b'K', 255, 255, 255, 255]),
    ] {
        assert!(AnimationDecoder::new(&data).is_err());
    }
    assert!(AnimationDecoder::new(&vec![0; MAX_INPUT_BYTES + 1]).is_err());
    let decoder = AnimationDecoder::new(include_bytes!("test_data/lossless-blend.webp")).unwrap();
    let frame = &decoder.frames[0];
    let payload = &decoder.data[frame.payload.clone()];
    assert!(validate_payload(payload, 0..payload.len(), 1, 1).is_err());
    let mut duplicated = payload.to_vec();
    duplicated.extend(payload);
    assert!(validate_payload(&duplicated, 0..duplicated.len(), frame.width, frame.height).is_err());
    for tag in [b"ANIM", b"ANMF", b"VP8X"] {
        let data = chunk(*tag, b"test");
        assert!(validate_payload(&data, 0..data.len(), 1, 1).is_err());
    }
    let data = chunk(*b"ALPH", b"");
    assert!(validate_payload(&data, 0..data.len(), 1, 1).is_err());
    let data = chunk(*b"JUNK", b"x");
    assert!(validate_payload(&data, 0..data.len(), 1, 1).is_err());
    assert!(codec_dimensions(*b"VP8L", b"12345").is_err());
    assert!(codec_dimensions(*b"VP8 ", b"1234567890").is_err());
    let mut payload = vec![0x2f, 0, 0, 0, 0xe0];
    assert!(codec_dimensions(*b"VP8L", &payload).is_err());
    payload[4] = 0;
    assert_eq!(codec_dimensions(*b"VP8L", &payload).unwrap(), (1, 1));
}

#[test]
fn codec_allocation_gate_and_callback_failure_preserve_live_decoder_state() {
    let bytes = include_bytes!("test_data/lossless-blend.webp");
    let mut decoder = AnimationDecoder::new(bytes).unwrap();
    AllocationBudget::new(0).run(|| assert!(decoder.next_frame(false, |_, _, _| Ok(())).is_err()));
    assert_eq!(decoder.next, 0);
    assert!(
        decoder
            .next_frame::<()>(false, |_, _, _| bail!("callback failure"))
            .is_err()
    );
    assert_eq!(
        decoder.next_frame(false, |_, _, time| Ok(time)).unwrap(),
        50
    );
}

#[test]
fn truncations_and_deterministic_bit_flips_never_panic() {
    let bytes = include_bytes!("test_data/lossy-alpha.webp");
    for n in [0, 4, 11, 12, 20, 40, bytes.len() - 1] {
        assert!(AnimationDecoder::new(&bytes[..n]).is_err());
    }
    for index in (12..bytes.len()).step_by(31) {
        let mut data = bytes.to_vec();
        data[index] ^= 0x80;
        if let Ok(mut decoder) = AnimationDecoder::new(&data) {
            let _ = decoder.next_frame(false, |_, _, _| Ok(()));
        }
    }
}

fn hex(value: &str) -> Vec<u8> {
    value
        .split_whitespace()
        .flat_map(|s| {
            s.as_bytes()
                .chunks_exact(2)
                .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
        })
        .collect()
}
#[test]
fn existing_animation_has_exact_timing_reset_and_owned_input() {
    let mut input = hex(include_str!("../../../../test_data/animated.webp.hex"));
    let mut decoder = AnimationDecoder::new(&input).unwrap();
    input.fill(0);
    let info = decoder.info();
    assert_eq!(
        (info.width, info.height, info.frame_count, info.loop_count),
        (400, 400, 10, 0)
    );
    for t in (40..=400).step_by(40) {
        decoder
            .next_frame(false, |frame, meta, time| {
                assert_eq!(frame.len(), 400 * 400 * 4);
                assert_eq!(meta, info);
                assert_eq!(time, t);
                Ok(())
            })
            .unwrap();
    }
    assert_eq!(decoder.next_frame(false, |_, _, t| Ok(t)).unwrap(), 40);
    assert_eq!(decoder.next_frame(true, |_, _, t| Ok(t)).unwrap(), 40);
}
#[test]
fn malformed_and_size_limits_fail_before_allocation() {
    for input in [b"".as_slice(), b"not a webp", b"RIFF\xFF\xFF\xFF\xFFWEBP"] {
        assert!(AnimationDecoder::new(input).is_err());
    }
    for (w, h) in [
        (0, 1),
        (1, 0),
        (u32::MAX, u32::MAX),
        (u32::try_from(MAX_CANVAS_BYTES).unwrap(), 1),
    ] {
        assert!(frame_length(w, h).is_err());
    }
}
