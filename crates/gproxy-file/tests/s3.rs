#![cfg(all(feature = "s3", not(target_arch = "wasm32")))]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use gproxy_file::{
    Buffer, ErrorKind, S3,
    opendal::{HttpBody, HttpTransport, HttpTransporter},
    s3_with_transport,
};
use http::{Request, Response};

#[derive(Clone, Debug, Default)]
struct S3Transport {
    objects: Arc<Mutex<HashMap<String, Vec<u8>>>>,
    requests: Arc<Mutex<Vec<(String, String, String)>>>,
}

impl HttpTransport for S3Transport {
    async fn fetch(&self, request: Request<Buffer>) -> gproxy_file::Result<Response<HttpBody>> {
        let path = request.uri().path().to_owned();
        let method = request.method().as_str().to_owned();
        let authorization = request
            .headers()
            .get("authorization")
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned();
        self.requests
            .lock()
            .unwrap()
            .push((method.clone(), path.clone(), authorization));
        let mut objects = self.objects.lock().unwrap();
        let (status, body, size) = match method.as_str() {
            "PUT" => {
                objects.insert(path, request.body().to_vec());
                (200, Vec::new(), 0)
            }
            "GET" => match objects.get(&path) {
                Some(data) => (200, data.clone(), data.len()),
                None => {
                    let body = b"<Error><Code>NoSuchKey</Code></Error>".to_vec();
                    let size = body.len();
                    (404, body, size)
                }
            },
            "HEAD" => match objects.get(&path) {
                Some(data) => (200, Vec::new(), data.len()),
                None => (404, Vec::new(), 0),
            },
            "DELETE" => {
                objects.remove(&path);
                (204, Vec::new(), 0)
            }
            _ => panic!("unexpected S3 method {method}"),
        };
        let body = HttpBody::new(futures_util::stream::iter([Ok(Buffer::from(body))]), None);
        Ok(Response::builder()
            .status(status)
            .header("content-length", size)
            .header("etag", "\"test-etag\"")
            .body(body)
            .unwrap())
    }
}

#[tokio::test]
async fn signed_s3_requests_roundtrip_through_the_injected_transport() {
    let transport = S3Transport::default();
    let files = s3_with_transport(
        S3::default()
            .bucket("test-bucket")
            .region("auto")
            .endpoint("https://test-account.r2.cloudflarestorage.com")
            .access_key_id("test-access-key")
            .secret_access_key("test-secret-key")
            .disable_config_load()
            .disable_ec2_metadata(),
        HttpTransporter::new(transport.clone()),
    )
    .unwrap();
    files
        .write("folder/a b.bin", vec![0, 128, 255])
        .await
        .unwrap();
    assert_eq!(
        files.read("folder/a b.bin").await.unwrap().to_vec(),
        [0, 128, 255]
    );
    assert_eq!(
        files.stat("folder/a b.bin").await.unwrap().content_length(),
        3
    );
    files.write("folder/a b.bin", Buffer::new()).await.unwrap();
    assert!(files.read("folder/a b.bin").await.unwrap().is_empty());
    files.delete("folder/a b.bin").await.unwrap();
    assert_eq!(
        files.stat("folder/a b.bin").await.unwrap_err().kind(),
        ErrorKind::NotFound
    );
    assert_eq!(
        files.read("folder/a b.bin").await.unwrap_err().kind(),
        ErrorKind::NotFound
    );
    let requests = transport.requests.lock().unwrap();
    assert!(
        requests
            .iter()
            .any(|(method, path, _)| method == "PUT" && path == "/test-bucket/folder/a%20b.bin")
    );
    assert!(requests.iter().all(|(_, _, authorization)| {
        authorization.starts_with("AWS4-HMAC-SHA256 ")
            && authorization.contains("/auto/s3/aws4_request")
    }));
}
