use super::*;
#[cfg(feature = "claude")]
pub fn claude() -> Value {
    json!({

            "type":"message",
        "id":"msg_1",
        "model":"claude",
        "role":"assistant",
        "stop_reason":"tool_use",

            "content":[

                {
                "type":"thinking",
                "thinking":"private thought",
                "signature":"claude-signature",
                "future":{
                    "keep":true
                }
            },

                {
                "type":"redacted_thinking",
                "data":"redacted-ciphertext",
                "opaque_extra":[
                    1,
                    2
                ]
            },

                {
                "type":"text",
                "text":"你好",
                "future_text":{
                    "a":1
                }
            },

                {
                "type":"tool_use",
                "id":"tool_1",
                "name":"weather",
                "input":{
                    "city":"上海"
                },
                "extra_tool":{
                    "x":1
                }
            }

        ],

            "usage":{
            "input_tokens":12,
            "output_tokens":8,
            "cache_read_input_tokens":3,
            "cache_creation_input_tokens":4,
            "output_tokens_details":{
                "thinking_tokens":2
            }
        }

    })
}

#[cfg(feature = "openai")]
pub fn openai() -> Value {
    json!({

            "id":"resp_1",
        "created_at":1,
        "model":"gpt",
        "object":"response",
        "status":"completed",

            "error":null,
        "incomplete_details":null,
        "instructions":null,
        "metadata":null,
        "parallel_tool_calls":true,

            "temperature":null,
        "top_p":null,
        "tool_choice":"auto",
        "tools":[

        ],

            "output":[

                {
                "type":"reasoning",
                "id":"rs_1",
                "summary":[
                    {
                        "type":"summary_text",
                        "text":"one",
                        "extension":1
                    },
                    {
                        "type":"summary_text",
                        "text":"two"
                    }
                ],
                "content":[
                    {
                        "type":"reasoning_text",
                        "text":"raw thought"
                    }
                ],
                "encrypted_content":"encrypted-reasoning",
                "future":{
                    "keep":true
                }
            },

                {
                "type":"message",
                "id":"msg_1",
                "role":"assistant",
                "status":"completed",
                "phase":"final_answer",
                "envelope_extra":7,
                "content":[

                    {
                        "type":"output_text",
                        "text":"你好",
                        "annotations":[

                        ],
                        "logprobs":[

                        ],
                        "future_part":{
                            "keep":1
                        }
                    },

                    {
                        "type":"output_text",
                        "text":" again",
                        "annotations":[

                        ],
                        "logprobs":[

                        ]
                    }

                ]
            },

                {
                "type":"function_call",
                "id":"fc_1",
                "call_id":"call_1",
                "name":"weather",
                "arguments":"{ \"city\": \"上海\" }",
                "status":"completed",
                "future_call":{
                    "x":1
                }
            },

                {
                "type":"compaction",
                "id":"cmp_1",
                "encrypted_content":"compaction-ciphertext",
                "future_compaction":true
            }

        ],

            "usage":{
            "input_tokens":12,
            "output_tokens":8,
            "total_tokens":20,
            "input_tokens_details":{
                "cached_tokens":3,
                "cache_write_tokens":4
            },
            "output_tokens_details":{
                "reasoning_tokens":2
            }
        }

    })
}

#[cfg(feature = "gemini")]
pub fn gemini() -> Value {
    json!({

            "candidates":[
            {
                "index":0,
                "finishReason":"STOP",
                "content":{
                    "role":"model",
                    "parts":[

                {
                            "text":"thought",
                            "thought":true,
                            "thoughtSignature":"gemini-thought-signature",
                            "future":{
                                "keep":true
                            }
                        },

                {
                            "text":"你好",
                            "thoughtSignature":"gemini-text-signature",
                            "partMetadata":{
                                "a":1
                            }
                        },

                {
                            "functionCall":{
                                "id":"gcall_1",
                                "name":"weather",
                                "args":{
                                    "city":"上海"
                                },
                                "futureArg":3
                            },
                            "thoughtSignature":"gemini-call-signature"
                        },

                {
                            "inlineData":{
                                "mimeType":"image/png",
                                "data":"aGVsbG8=",
                                "displayName":"picture",
                                "futureData":7
                            },
                            "thoughtSignature":"gemini-image-signature"
                        },

                {
                            "fileData":{
                                "mimeType":"application/pdf",
                                "fileUri":"https://example.test/doc.pdf",
                                "displayName":"doc",
                                "futureFile":7
                            }
                        },

            {
                            "executableCode":{
                                "language":"PYTHON",
                                "code":"print(1)"
                            },
                            "futurePart":true
                        },

            {
                            "fileData":{
                                "fileUri":"gs://bucket/document"
                            },
                            "futurePart":3
                        },

            {
                            "functionCall":{
                                "name":"no_arguments"
                            },
                            "thoughtSignature":"gemini-empty-call-signature"
                        },

            {
                            "functionResponse":{
                                "name":"weather",
                                "response":{
                                    "result":"sunny",
                                    "futureResult":1
                                },
                                "futureResponse":2
                            }
                        }

                    ]
                }
            }
        ],

            "usageMetadata":{
            "promptTokenCount":12,
            "candidatesTokenCount":8,
            "thoughtsTokenCount":2,
            "cachedContentTokenCount":3,
            "totalTokenCount":20
        }

    })
}
