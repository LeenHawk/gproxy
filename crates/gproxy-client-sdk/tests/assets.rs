#![cfg(all(
    feature = "asset-upload",
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini"
))]
mod support;
use gproxy_client_sdk::*;
use protocol::{
    HttpBody, OperationKey, WireRequest, WireResponse, capability::CapabilityFuture,
    connection::Bytes,
};
use serde_json::{Value, json};
use std::{collections::VecDeque, sync::Mutex};
struct Reply {
    headers: http::HeaderMap,
    body: Value,
    status: http::StatusCode,
}
struct Recorded {
    key: OperationKey,
    model: Option<String>,
    path: String,
    headers: http::HeaderMap,
    body: Bytes,
}
struct Script {
    replies: Mutex<VecDeque<Reply>>,
    seen: Mutex<Vec<Recorded>>,
    endpoint: Option<String>,
}
impl Script {
    fn new(values: Vec<Value>) -> Self {
        Self {
            replies: Mutex::new(
                values
                    .into_iter()
                    .map(|body| Reply {
                        body,
                        headers: http::HeaderMap::new(),
                        status: http::StatusCode::OK,
                    })
                    .collect(),
            ),
            seen: Mutex::new(Vec::new()),
            endpoint: None,
        }
    }
}
impl Backend for Script {
    fn endpoint(&self) -> Option<&str> {
        self.endpoint.as_deref()
    }
    fn send(
        &self,
        key: OperationKey,
        model: Option<&str>,
        request: WireRequest<HttpBody>,
    ) -> CapabilityFuture<'_, Result<WireResponse<HttpBody>>> {
        let model = model.map(str::to_owned);
        Box::pin(async move {
            let bytes = protocol::codec::read_http_body(request.body, DEFAULT_LIMITS)
                .await
                .unwrap();
            self.seen.lock().unwrap().push(Recorded {
                key,
                model,
                path: request.path,
                headers: request.headers,
                body: bytes,
            });
            let reply = self
                .replies
                .lock()
                .unwrap()
                .pop_front()
                .expect("unexpected upload request");
            Ok(WireResponse {
                status: reply.status,
                headers: reply.headers,
                body: HttpBody::Bytes(reply.body.to_string().into()),
            })
        })
    }
}
fn pdf(dialect: Dialect) -> GenerateRequest {
    support::request(
        dialect,
        vec![Message {
            role: Role::User,
            content: vec![Block::new(Content::Document(Document {
                source: DocumentSource::Asset {
                    source: Asset::Reference {
                        reference: "blob:pdf".into(),
                    },
                },
                media_type: Some("application/pdf".into()),
                title: Some("report.pdf".into()),
                context: None,
                citations: None,
                detail: Detail::Auto,
            }))],
        }],
    )
}
async fn resolve(reference: String) -> Result<assets::ResolvedAsset> {
    assert_eq!(reference, "blob:pdf");
    Ok(assets::ResolvedAsset::Bytes {
        bytes: b"%PDF evidence".to_vec(),
        media_type: "application/pdf".into(),
    })
}
async fn never(_: String) -> Result<assets::ResolvedAsset> {
    panic!("resolved assets must be reused")
}
fn openai_file(id: &str) -> Value {
    json!({"id":id,"bytes":13,"created_at":1,"filename":"report.pdf","object":"file","purpose":"user_data","status":"processed"})
}
#[test]
fn upload_cache_is_vendor_scoped_reused_and_reuploaded_after_expiry() {
    futures_executor::block_on(async {
        let client = Client::new(Script::new(vec![
            openai_file("file_1"),
            openai_file("file_2"),
            json!({"id":"claude_1","created_at":"2026-11-02T00:00:00Z","filename":"report.pdf","mime_type":"application/pdf","size_bytes":13,"type":"file"}),
        ]));
        let mut request = pdf(Dialect::Openai);
        let opts = assets::PreparationOptions {
            inline_bytes: Some(0),
        };
        client
            .prepare_assets_with(&mut request, opts, resolve)
            .await
            .unwrap();
        let block = &request.messages[0].content[0];
        assert_eq!(
            assets::uploaded(block, Vendor::Openai).unwrap().unwrap().id,
            "file_1"
        );
        assert!(matches!(
            &block.content,
            Content::Document(Document {
                source: DocumentSource::Asset {
                    source: Asset::Inline { .. }
                },
                ..
            })
        ));
        assert_eq!(
            dialects::render(&request, false).unwrap()["input"][0]["content"][0]["file_id"],
            "file_1"
        );
        client
            .prepare_assets_with(&mut request, opts, never)
            .await
            .unwrap();
        assert_eq!(client.backend.seen.lock().unwrap().len(), 1);
        request.messages[0].content[0]
            .provider_metadata
            .get_mut(&Vendor::Openai)
            .unwrap()
            .get_mut("upload")
            .unwrap()["expires_at"] = json!(1);
        assert!(dialects::render(&request, false).is_err());
        client
            .prepare_assets_with(&mut request, opts, never)
            .await
            .unwrap();
        assert_eq!(
            assets::uploaded(&request.messages[0].content[0], Vendor::Openai)
                .unwrap()
                .unwrap()
                .id,
            "file_2"
        );
        request.model.dialect = Dialect::Claude;
        client
            .prepare_assets_with(&mut request, opts, never)
            .await
            .unwrap();
        assert_eq!(
            dialects::render(&request, false).unwrap()["messages"][0]["content"][0]["source"]["file_id"],
            "claude_1"
        );
        let requests = client.backend.seen.lock().unwrap();
        assert_eq!(requests.len(), 3);
        assert_eq!(requests[0].key.operation, protocol::Operation::CreateFile);
        assert_eq!(
            requests[0].model.as_deref(),
            Some(request.model.id.as_str())
        );
        assert_eq!(requests[0].path, "/v1/files");
        assert!(String::from_utf8_lossy(&requests[0].body).contains("%PDF evidence"));
        assert!(requests[2].headers.contains_key("anthropic-beta"));
    });
}
#[test]
fn gemini_resumable_upload_stays_on_gateway_mount_and_reuses_the_uri() {
    futures_executor::block_on(async {
        let mut backend = Script::new(vec![
            json!(null),
            json!({"file":{"name":"files/file_1","uri":"https://generativelanguage.googleapis.com/v1beta/files/file_1","expirationTime":"2099-01-01T00:00:00Z","state":"ACTIVE"}}),
        ]);
        backend.endpoint = Some("http://localhost:8080/mount".into());
        backend.replies.lock().unwrap()[0].headers.insert(
            "x-goog-upload-url",
            http::HeaderValue::from_static(
                "http://localhost:8080/mount/upload/v1beta/files?upload_id=gproxy-session",
            ),
        );
        let client = Client::new(backend);
        let mut request = pdf(Dialect::Gemini);
        client
            .prepare_assets_with(
                &mut request,
                assets::PreparationOptions {
                    inline_bytes: Some(0),
                },
                resolve,
            )
            .await
            .unwrap();
        let native = dialects::render(&request, false).unwrap();
        assert_eq!(
            native["contents"][0]["parts"][0]["fileData"]["fileUri"],
            "https://generativelanguage.googleapis.com/v1beta/files/file_1"
        );
        let seen = client.backend.seen.lock().unwrap();
        assert_eq!(seen.len(), 2);
        assert_eq!(seen[0].path, "/upload/v1beta/files");
        assert_eq!(seen[1].path, "/upload/v1beta/files");
        assert_eq!(seen[1].body, b"%PDF evidence"[..]);
        assert_eq!(seen[1].headers["x-goog-upload-command"], "upload, finalize");
    });
}
#[test]
fn gemini_processing_id_is_saved_and_refreshed_without_uploading_again() {
    futures_executor::block_on(async {
        let backend = Script::new(vec![
            json!(null),
            json!({"file":{"name":"files/file_1","uri":"https://example.com/file_1","state":"PROCESSING"}}),
            json!({"name":"files/file_1","uri":"https://example.com/file_1","state":"ACTIVE"}),
        ]);
        backend.replies.lock().unwrap()[0].headers.insert(
            "x-goog-upload-url",
            http::HeaderValue::from_static("/upload/v1beta/files?upload_id=gproxy-session"),
        );
        let client = Client::new(backend);
        let mut request = pdf(Dialect::Gemini);
        let opts = assets::PreparationOptions {
            inline_bytes: Some(0),
        };
        assert!(
            client
                .prepare_assets_with(&mut request, opts, resolve)
                .await
                .is_err()
        );
        assert!(
            assets::uploaded(&request.messages[0].content[0], Vendor::Gemini)
                .unwrap()
                .unwrap()
                .processing
        );
        client
            .prepare_assets_with(&mut request, opts, never)
            .await
            .unwrap();
        let seen = client.backend.seen.lock().unwrap();
        assert_eq!(seen.len(), 3);
        assert_eq!(seen[2].key.operation, protocol::Operation::RetrieveFile);
        assert_eq!(seen[2].path, "/v1beta/files/file_1");
    });
}
#[test]
fn format_limits_and_cross_vendor_ids_fail_without_transport() {
    futures_executor::block_on(async {
        let client = Client::new(Script::new(vec![]));
        let mut request = support::request(
            Dialect::Claude,
            vec![Message {
                role: Role::User,
                content: vec![Block::new(Content::Image(Image {
                    source: Asset::Inline { bytes: vec![0] },
                    media_type: Some("image/svg+xml".into()),
                    detail: Detail::Auto,
                }))],
            }],
        );
        assert!(client.prepare_assets(&mut request, never).await.is_err());
        if let Content::Image(image) = &mut request.messages[0].content[0].content {
            image.media_type = Some("image/png".into());
            image.source = Asset::Inline {
                bytes: vec![0; 5 * 1024 * 1024 + 1],
            };
        }
        assert!(client.prepare_assets(&mut request, never).await.is_err());
        if let Content::Image(image) = &mut request.messages[0].content[0].content {
            image.source = Asset::ProviderFile {
                vendor: Vendor::Openai,
                id: "foreign".into(),
            };
        }
        assert!(client.prepare_assets(&mut request, never).await.is_err());
        assert!(client.backend.seen.lock().unwrap().is_empty());
    });
}
#[test]
fn upload_errors_keep_status_body_and_do_not_retry() {
    futures_executor::block_on(async {
        let backend = Script::new(vec![json!({"error":{"message":"slow"}})]);
        backend.replies.lock().unwrap()[0].status = http::StatusCode::TOO_MANY_REQUESTS;
        backend.replies.lock().unwrap()[0]
            .headers
            .insert("retry-after", http::HeaderValue::from_static("7"));
        let client = Client::new(backend);
        let mut request = pdf(Dialect::Openai);
        let error = client
            .prepare_assets_with(
                &mut request,
                assets::PreparationOptions {
                    inline_bytes: Some(0),
                },
                resolve,
            )
            .await
            .unwrap_err();
        assert!(matches!(error,Error::RateLimit {retry_after:Some(value),..} if value=="7"));
        assert_eq!(client.backend.seen.lock().unwrap().len(), 1);
    });
}
