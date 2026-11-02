use gproxy_client_sdk::*;
use gproxy_protocol::{
    HttpBody, OperationKey, WireRequest, WireResponse, capability::CapabilityFuture,
    connection::Bytes,
};
use http::{Method, StatusCode};
use serde_json::{Value, json};
use std::{collections::VecDeque, sync::Mutex};

#[derive(Debug)]
struct Recorded {
    key: OperationKey,
    model: Option<String>,
    method: Method,
    path: String,
    query: Option<String>,
    headers: http::HeaderMap,
    body: Bytes,
}
struct BackendScript {
    replies: Mutex<VecDeque<Bytes>>,
    requests: Mutex<Vec<Recorded>>,
}
impl BackendScript {
    fn new(replies: Vec<Value>) -> Self {
        Self {
            replies: Mutex::new(replies.into_iter().map(|v| v.to_string().into()).collect()),
            requests: Mutex::new(vec![]),
        }
    }
    fn take(&self) -> Recorded {
        self.requests.lock().unwrap().remove(0)
    }
}
impl Backend for BackendScript {
    fn send(
        &self,
        key: OperationKey,
        model: Option<&str>,
        request: WireRequest<HttpBody>,
    ) -> CapabilityFuture<'_, Result<WireResponse<HttpBody>>> {
        let model = model.map(str::to_owned);
        Box::pin(async move {
            let body = gproxy_protocol::codec::read_http_body(request.body, DEFAULT_LIMITS)
                .await
                .unwrap();
            self.requests.lock().unwrap().push(Recorded {
                key,
                model,
                method: request.method,
                path: request.path,
                query: request.query,
                headers: request.headers,
                body,
            });
            Ok(WireResponse {
                status: StatusCode::OK,
                headers: http::HeaderMap::new(),
                body: HttpBody::Bytes(self.replies.lock().unwrap().pop_front().unwrap()),
            })
        })
    }
}
fn params<T: serde::de::DeserializeOwned>(value: Value) -> operations::Params<T> {
    let mut params =
        operations::Params::new(serde_json::from_value(value).unwrap(), Vendor::Claude);
    params.provider_options.insert(
        Vendor::Claude,
        serde_json::from_value(json!({"vendor_extension":{"enabled":true}})).unwrap(),
    );
    params.provider_options.insert(
        Vendor::Gemini,
        serde_json::from_value(json!({"wrong_vendor":true})).unwrap(),
    );
    params
}
fn assert_request(request: Recorded, path: &str) -> Value {
    assert_eq!(request.method, Method::POST);
    assert_eq!(request.path, path);
    assert_eq!(request.model.as_deref(), Some("model"));
    let body: Value = serde_json::from_slice(&request.body).unwrap();
    assert_eq!(body["vendor_extension"], json!({"enabled":true}));
    assert!(body.get("wrong_vendor").is_none());
    body
}

#[cfg(feature = "embeddings")]
#[test]
fn embeddings_use_typed_native_params_and_response() {
    futures_executor::block_on(async {
        let client = Client::new(BackendScript::new(vec![json!({
            "object":"list",
            "model":"model",
            "data":[
                {
                    "object":"embedding",
                    "index":0,
                    "embedding":[
                        0.1,
                        0.2
                    ]
                }
            ],
            "usage":{
                "prompt_tokens":1,
                "total_tokens":1
            }
        })]));
        let result = client
            .embeddings(&params(
                json!({"model":"model","input":["hello"],"dimensions":2}),
            ))
            .await
            .unwrap();
        assert_eq!(result.data.len(), 1);
        assert_eq!(
            assert_request(client.backend.take(), "/v1/embeddings")["input"],
            json!(["hello"])
        );
    });
}
#[cfg(feature = "rerank")]
#[test]
fn rerank_uses_typed_native_params_and_response() {
    futures_executor::block_on(async {
        let client = Client::new(BackendScript::new(vec![
            json!({"model":"model","results":[]}),
        ]));
        assert!(
            client
                .rerank(&params(
                    json!({"model":"model","query":"q","documents":["a","b"],"top_n":1})
                ))
                .await
                .unwrap()
                .results
                .is_empty()
        );
        assert_eq!(
            assert_request(client.backend.take(), "/v1/rerank")["documents"],
            json!(["a", "b"])
        );
    });
}
#[cfg(feature = "images")]
#[test]
fn image_generation_and_json_edit_preserve_options_and_sources() {
    futures_executor::block_on(async {
        let reply = json!({"created":1,"data":[{"b64_json":"aGk="}]});
        let client = Client::new(BackendScript::new(vec![reply.clone(), reply]));
        client
            .create_image(&params(json!({"model":"model","prompt":"draw"})))
            .await
            .unwrap();
        assert_request(client.backend.take(), "/v1/images/generations");
        client.edit_image(&params(json!({"model":"model","prompt":"edit","images":[{"image_url":"data:image/png;base64,aGk="}]}))).await.unwrap();
        assert_eq!(
            assert_request(client.backend.take(), "/v1/images/edits")["images"][0]["image_url"],
            "data:image/png;base64,aGk="
        );
    });
}
#[cfg(feature = "models")]
#[test]
fn model_listing_has_no_model_constraint_or_upstream_discovery() {
    futures_executor::block_on(async {
        let client = Client::new(BackendScript::new(vec![
            json!({"object":"list","data":[{"object":"model","id":"p/m","created":1,"owned_by":"local"}]}),
        ]));
        assert_eq!(client.list_models().await.unwrap().data[0].id, "p/m");
        let request = client.backend.take();
        assert_eq!(
            request.key.operation,
            gproxy_protocol::Operation::ListModels
        );
        assert_eq!(request.method, Method::GET);
        assert!(request.model.is_none());
        assert!(request.body.is_empty());
    });
}
#[cfg(feature = "video")]
#[test]
fn video_is_an_async_job_without_sdk_polling_and_resource_ids_are_encoded() {
    futures_executor::block_on(async {
        let video = json!({"id":"vid/1","object":"video","created_at":1,"model":"model","progress":0,"seconds":"4","size":"720x1280","status":"queued"});
        let client = Client::new(BackendScript::new(vec![
            video.clone(),
            video.clone(),
            json!({"object":"list","data":[video],"has_more":false}),
            json!({"id":"vid/1","object":"video.deleted","deleted":true}),
            json!("bytes"),
        ]));
        let created = client
            .create_video(&params(
                json!({"model":"model","prompt":"motion","seconds":"4"}),
            ))
            .await
            .unwrap();
        assert_eq!(created.progress, 0);
        assert_request(client.backend.take(), "/v1/videos");
        client.retrieve_video("vid/1").await.unwrap();
        let request = client.backend.take();
        assert_eq!(request.path, "/v1/videos/vid%2F1");
        assert!(request.model.is_none());
        let query = serde_json::from_value(json!({"limit":1})).unwrap();
        client.list_videos(&query).await.unwrap();
        assert_eq!(client.backend.take().query.as_deref(), Some("limit=1"));
        client.delete_video("vid/1").await.unwrap();
        assert_eq!(client.backend.take().method, Method::DELETE);
        client.download_video("vid/1").await.unwrap();
        assert_eq!(client.backend.take().path, "/v1/videos/vid%2F1/content");
    });
}
#[cfg(all(feature = "audio", feature = "openai"))]
#[test]
fn speech_retains_raw_audio_and_transcription_encodes_a_real_multipart_upload() {
    futures_executor::block_on(async {
        let client = Client::new(BackendScript::new(vec![
            json!("audio"),
            json!({"text":"recognized"}),
        ]));
        let response = client
            .speech(&params(
                json!({"model":"model","input":"speak","voice":"alloy","response_format":"wav"}),
            ))
            .await
            .unwrap();
        assert!(matches!(response.body, HttpBody::Bytes(_)));
        assert_request(client.backend.take(), "/v1/audio/speech");
        let request = operations::TranscriptionParams {
            model: "model".into(),
            vendor: Vendor::Openai,
            bytes: vec![0, 1, 2, 255],
            filename: "voice.wav".into(),
            media_type: "audio/wav".into(),
            language: Some("zh".into()),
            provider_options: Default::default(),
        };
        client.transcribe(&request).await.unwrap();
        let recorded = client.backend.take();
        assert_eq!(recorded.path, "/v1/audio/transcriptions");
        let content_type = recorded.headers["content-type"].to_str().unwrap();
        let boundary = content_type
            .strip_prefix("multipart/form-data; boundary=")
            .unwrap();
        let mut decoder = gproxy_protocol::codec::MultipartDecoder::new(
            HttpBody::Bytes(recorded.body),
            boundary,
            DEFAULT_LIMITS,
        )
        .unwrap();
        let mut fields = Vec::new();
        while let Some(part) = decoder.next_part().await.unwrap() {
            let disposition = part.headers["content-disposition"]
                .to_str()
                .unwrap()
                .to_owned();
            let body = gproxy_protocol::codec::read_http_body(part.body, DEFAULT_LIMITS)
                .await
                .unwrap();
            fields.push((disposition, body));
        }
        assert!(
            fields
                .iter()
                .any(|(h, b)| h.contains("filename=\"voice.wav\"") && b.as_ref() == [0, 1, 2, 255])
        );
        assert!(
            fields
                .iter()
                .any(|(h, b)| h.contains("name=\"model\"") && b == "model")
        );
        assert!(
            fields
                .iter()
                .any(|(h, b)| h.contains("name=\"language\"") && b == "zh")
        );
    });
}
