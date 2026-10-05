use super::*;
#[test]
fn interval_hit_and_fifo_limit() {
    for elapsed in [-1, 0, 1, 2_999, 3_000, 3_001, i64::MAX, i64::MIN] {
        let request = RecoverySearchRequest {
            version: 1,
            cached: false,
            elapsed_millis: elapsed,
            cache_size: 127,
            has_results: false,
        };
        let result = build_envelope(&request).payload.unwrap();
        assert_eq!(
            result.wait_millis,
            3_000_i64.saturating_sub(elapsed).clamp(0, 3_000)
        );
        assert!(!result.store);
        assert_eq!(result.evict_count, 0);
        let request = RecoverySearchRequest {
            cached: true,
            has_results: true,
            ..request
        };
        assert_eq!(
            build_envelope(&request).payload.unwrap(),
            RecoverySearchPlan {
                use_cache: true,
                wait_millis: 0,
                store: false,
                evict_count: 0
            }
        );
    }
    for size in [0, 127, 128] {
        let request = RecoverySearchRequest {
            version: 1,
            cached: false,
            elapsed_millis: 3_000,
            cache_size: size,
            has_results: true,
        };
        let result = build_envelope(&request);
        assert_eq!(
            result.payload.as_ref().unwrap().evict_count,
            i32::from(size == 128)
        );
        assert!(result.payload.as_ref().unwrap().store);
        assert_eq!(
            serde_cbor::from_slice::<ApiEnvelope<RecoverySearchPlan>>(
                &serde_cbor::to_vec(&result).unwrap()
            )
            .unwrap(),
            result
        );
        let _: RecoverySearchRequest =
            serde_cbor::from_slice(&serde_cbor::to_vec(&request).unwrap()).unwrap();
    }
    for (version, size) in [(2, 0), (1, -1), (1, 129)] {
        assert!(
            !build_envelope(&RecoverySearchRequest {
                version,
                cached: false,
                elapsed_millis: 0,
                cache_size: size,
                has_results: false
            })
            .is_ok()
        );
    }
}
