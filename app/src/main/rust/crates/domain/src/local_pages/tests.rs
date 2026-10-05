use super::*;
#[test]
fn names_and_stable_reader_order_preserve_both_policies() {
    for extension in IMAGE_EXTENSIONS {
        assert_eq!(
            page_number(&format!("00000001.{extension}"), false, false),
            Some(1)
        );
        assert_eq!(
            page_number(
                &format!("00000000.{}", extension.to_uppercase()),
                true,
                true
            ),
            Some(0)
        );
    }
    for name in [
        "folder/00000001.jpg",
        "folder\\00000001.jpg",
        "1.jpg",
        "0000000x.jpg",
        "00000001.txt",
        "00000001",
        "00000001.jpg.tmp",
    ] {
        assert_eq!(page_number(name, true, true), None);
    }
    assert_eq!(page_number("00000000.jpg", false, false), None);
    assert_eq!(page_number("00000001.JPG", false, false), None);
    let request = LocalPagesRequest {
        version: 1,
        names: [
            "00000002.jpg",
            "junk",
            "00000001.JPG",
            "00000001.jpg",
            "00000000.png",
        ]
        .map(str::to_owned)
        .to_vec(),
    };
    let result = build_envelope(&request);
    assert_eq!(
        result
            .payload
            .as_ref()
            .unwrap()
            .iter()
            .map(|page| page.source_index)
            .collect::<Vec<_>>(),
        vec![4, 2, 3, 0]
    );
    assert_eq!(result.payload.as_ref().unwrap()[1].extension, "JPG");
    assert_eq!(
        serde_cbor::from_slice::<ApiEnvelope<Vec<LocalPage>>>(
            &serde_cbor::to_vec(&result).unwrap()
        )
        .unwrap(),
        result
    );
    let _: LocalPagesRequest =
        serde_cbor::from_slice(&serde_cbor::to_vec(&request).unwrap()).unwrap();
}
#[test]
fn empty_version_count_and_name_budgets() {
    assert!(
        build_envelope(&LocalPagesRequest {
            version: 1,
            names: vec![]
        })
        .payload
        .unwrap()
        .is_empty()
    );
    for request in [
        LocalPagesRequest {
            version: 2,
            names: vec![],
        },
        LocalPagesRequest {
            version: 1,
            names: vec![String::new(); 100_001],
        },
        LocalPagesRequest {
            version: 1,
            names: vec!["x".repeat(4_097)],
        },
    ] {
        assert!(!build_envelope(&request).is_ok());
    }
}
