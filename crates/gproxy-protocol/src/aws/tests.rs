use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};

use super::{ConverseResponse, ImageBlockDelta};

#[test]
fn converse_trace_and_image_error_round_trip_open_extensions() {
    round_trip::<ConverseResponse>(json!({
        "output": {"message": {"role": "assistant", "content": []}},
        "stopReason": "end_turn",
        "usage": {"inputTokens": 2, "outputTokens": 1, "totalTokens": 3},
        "metrics": {"latencyMs": 7},
        "trace": {
            "guardrail": {
                "inputAssessment": {
                    "content-1": {
                        "automatedReasoningPolicy": {
                            "findings": [
                                {
                                    "valid": {
                                        "claimsTrueScenario": {
                                            "statements": [{
                                                "logic": "claim_1",
                                                "naturalLanguage": "the claim",
                                                "statementExtension": true
                                            }],
                                            "scenarioExtension": "kept"
                                        },
                                        "logicWarning": {"type": "FUTURE_WARNING"},
                                        "supportingRules": [{
                                            "identifier": "rule-1",
                                            "policyVersionArn": "arn:aws:bedrock:rule-1"
                                        }],
                                        "findingExtension": 1
                                    },
                                    "unionExtension": "kept"
                                },
                                {"futureFinding": {"shape": 1}}
                            ],
                            "policyExtension": false
                        },
                        "contentPolicy": {
                            "filters": [{
                                "action": "BLOCKED",
                                "confidence": "HIGH",
                                "type": "FUTURE_FILTER",
                                "filterExtension": [1, 2]
                            }]
                        },
                        "assessmentExtension": null
                    }
                },
                "guardrailExtension": {"nested": true}
            },
            "promptRouter": {
                "invokedModelId": "router-model",
                "routerExtension": "kept"
            },
            "traceExtension": 9
        },
        "responseExtension": "kept"
    }));

    round_trip::<ImageBlockDelta>(json!({
        "error": {
            "message": "image generation failed",
            "errorExtension": {"code": "future"}
        },
        "deltaExtension": true
    }));
}

fn round_trip<T>(wire: Value)
where
    T: DeserializeOwned + Serialize,
{
    let decoded = serde_json::from_value::<T>(wire.clone()).expect("decode AWS wire value");
    assert_eq!(
        serde_json::to_value(decoded).expect("encode AWS wire value"),
        wire
    );
}
