use super::*;

#[test]
fn frozen_jvm_reference_matches_all_1536_utf16_retention_and_candidate_cases() {
    for (case, line) in include_str!("../legacy_oracle.jsonl").lines().enumerate() {
        let value: serde_json::Value = serde_json::from_str(line).unwrap();
        let operation: StorageOperation =
            serde_json::from_value(value["operation"].clone()).unwrap();
        let mut req = request(operation);
        let expected: Vec<u32> = serde_json::from_value(value["expected"].clone()).unwrap();
        let raw_units = |hex: &str| {
            hex.as_bytes()
                .chunks_exact(4)
                .flat_map(|unit| {
                    u16::from_str_radix(std::str::from_utf8(unit).unwrap(), 16)
                        .unwrap()
                        .to_le_bytes()
                })
                .collect::<Vec<_>>()
        };
        match operation {
            StorageOperation::Names => {
                req.names = value["utf16"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|name| NameInput {
                        utf16: raw_units(name.as_str().unwrap()).into(),
                    })
                    .collect();
            }
            StorageOperation::Cleanup => {
                req.modified = serde_json::from_value(value["modified"].clone()).unwrap();
                req.max_items = serde_json::from_value(value["maxItems"].clone()).unwrap();
            }
            StorageOperation::Candidates => {
                req.max_items = serde_json::from_value(value["maxItems"].clone()).unwrap();
                req.candidates = value["utf16"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .enumerate()
                    .map(|(index, name)| CandidateInput {
                        index: u32::try_from(index).unwrap(),
                        kind: serde_json::from_value(value["kinds"][index].clone()).unwrap(),
                        source_id_utf16: raw_units(name.as_str().unwrap()).into(),
                    })
                    .collect();
            }
            StorageOperation::Classify => unreachable!(),
        }
        let plan = plan(&req).unwrap();
        if operation == StorageOperation::Candidates {
            assert_eq!(
                plan.candidates
                    .iter()
                    .map(|candidate| candidate.index)
                    .collect::<Vec<_>>(),
                expected,
                "case {case}"
            );
            assert_eq!(
                plan.limit_exceeded,
                value["limitExceeded"].as_bool().unwrap(),
                "case {case}"
            );
        } else {
            assert_eq!(plan.indices, expected, "case {case}");
        }
    }
}
