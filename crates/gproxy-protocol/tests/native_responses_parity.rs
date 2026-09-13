//! Shared with the actual v2/v3 historical public-collector replay.
use gproxy_protocol::transform::generate::stream::responses::ResponsesStreamCollector;
use serde_json::Value;

#[test]
fn historical_native_responses_sequences_reject_contradictory_lifecycles() {
    let cases: Vec<Value> =
        serde_json::from_str(include_str!("fixtures/native_responses_parity.json")).unwrap();
    assert_eq!(cases.len(), 10);
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let mut collector = ResponsesStreamCollector::new(Default::default());
        let mut failed = false;
        for event in case["events"].as_array().unwrap() {
            if collector
                .push(serde_json::from_value(event.clone()).unwrap())
                .is_err()
            {
                failed = true;
                break;
            }
        }
        let result = collector.finish();
        assert_eq!(!failed && result.is_ok(), name == "valid_text", "{name}");
        if let Ok(response) = result {
            assert_eq!(
                serde_json::to_value(response.value).unwrap()["output"][0]["content"][0]["text"],
                "Hello"
            );
        }
    }
}
