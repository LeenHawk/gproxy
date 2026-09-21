use gproxy_protocol::{transform::video::*, wire::openai::video as o};
use serde_json::{Value, json};
use std::collections::BTreeMap;
fn run(case: &Value) -> Result<Value, String> {
    let input = case["input"].clone();
    match case["direction"].as_str().unwrap() {
        "native_request" => serde_json::to_value(
            native_to_gemini_request(
                serde_json::from_value(input).map_err(|e| e.to_string())?,
                "veo",
                None,
                &BTreeMap::new(),
            )
            .map_err(|e| e.to_string())?
            .value
            .body,
        ),
        "veo_request" => serde_json::to_value(
            gemini_to_native_request(
                serde_json::from_value(input).map_err(|e| e.to_string())?,
                "sora-2",
                None,
                &BTreeMap::new(),
            )
            .map_err(|e| e.to_string())?
            .value
            .body,
        ),
        "veo_response" => {
            let facts = NativeVideoResponseFacts {
                operation_name: "operations/a".into(),
                client_id: "video_host".into(),
                created_at: 123,
                model: "sora-2".into(),
                seconds: o::NativeVideoSeconds::Eight,
                size: o::NativeVideoSize::Landscape720,
                progress: if input["done"] == true { 100 } else { 0 },
                pending_status: NativePendingStatus::InProgress,
                completed_at: None,
                expires_at: None,
                prompt: Some("sunset".into()),
                failure_code: None,
            };
            serde_json::to_value(
                gemini_operation_to_native(
                    serde_json::from_value(input).map_err(|e| e.to_string())?,
                    &facts,
                )
                .map_err(|e| e.to_string())?
                .value
                .body,
            )
        }
        "native_response" => serde_json::to_value(
            native_to_gemini_operation(
                serde_json::from_value(input).map_err(|e| e.to_string())?,
                NativeToVeoContext {
                    source_id: "video_a".into(),
                    operation_name: "operations/host-a".into(),
                    video: Some(
                        serde_json::from_value(json!({"uri":"https://published.test/actual.mp4"}))
                            .unwrap(),
                    ),
                },
            )
            .map_err(|e| e.to_string())?
            .value,
        ),
        _ => unreachable!(),
    }
    .map_err(|e| e.to_string())
}

#[test]
fn actual_shared_video_fixtures_match_recorded_v4_semantics() {
    let cases: Value =
        serde_json::from_str(include_str!("fixtures/native_video_parity.json")).unwrap();
    let records: Value =
        serde_json::from_str(include_str!("fixtures/native_video_parity_results.json")).unwrap();
    for case in cases.as_array().unwrap() {
        let expected = records["runs"]["v4"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["name"] == case["name"])
            .unwrap();
        match run(case) {
            Ok(value) => {
                assert_eq!(expected["accepted"], true, "{}", case["name"]);
                assert_eq!(value, expected["output"], "{}", case["name"]);
            }
            Err(_) => assert_eq!(expected["accepted"], false, "{}", case["name"]),
        }
    }
}
