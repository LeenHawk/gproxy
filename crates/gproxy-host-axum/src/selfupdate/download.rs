use sha2::{Digest as _, Sha256};

use super::manifest::{Artifact, Manifest};
use super::{Error, Result};

pub(super) async fn manifest(client: &wreq::Client, url: &str) -> Result<Manifest> {
    let response = client.get(url).send().await.map_err(|_| Error::Download)?;
    if !response.status().is_success() {
        return Err(Error::Download);
    }
    let bytes = response.bytes().await.map_err(|_| Error::Download)?;
    Manifest::parse_verified(&bytes)
}

pub(super) async fn artifact(client: &wreq::Client, artifact: &Artifact) -> Result<Vec<u8>> {
    let response = client
        .get(&artifact.url)
        .send()
        .await
        .map_err(|_| Error::Download)?;
    if !response.status().is_success() {
        return Err(Error::Download);
    }
    let bytes = response.bytes().await.map_err(|_| Error::Download)?;
    if bytes.len() as u64 != artifact.size {
        return Err(Error::Integrity);
    }
    let actual = hex(&Sha256::digest(&bytes));
    if !actual.eq_ignore_ascii_case(artifact.sha256.trim()) {
        return Err(Error::Integrity);
    }
    Ok(bytes.to_vec())
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(output, "{byte:02x}").expect("writing to String cannot fail");
    }
    output
}

#[cfg(test)]
mod tests {
    use axum::{Router, routing::get};
    use http::{StatusCode, header::LOCATION};
    use sha2::{Digest as _, Sha256};

    use super::{Artifact, Error};
    use crate::selfupdate::Manager;

    #[tokio::test]
    async fn update_client_follows_release_redirects_without_skipping_verification() {
        let router = Router::new()
            .route(
                "/release",
                get(|| async { (StatusCode::FOUND, [(LOCATION, "/archive")]) }),
            )
            .route("/archive", get(|| async { "verified update bytes" }))
            .route(
                "/manifest",
                get(|| async { (StatusCode::FOUND, [(LOCATION, "/invalid-manifest")]) }),
            )
            .route("/invalid-manifest", get(|| async { "{}" }))
            .route(
                "/loop",
                get(|| async { (StatusCode::FOUND, [(LOCATION, "/loop")]) }),
            );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind update fixture");
        let address = listener.local_addr().expect("fixture address");
        let server = tokio::spawn(async move {
            axum::serve(listener, router).await.expect("serve fixture");
        });
        let directory = tempfile::tempdir().expect("update directory");
        let manager =
            Manager::new(directory.path().to_owned(), None, None).expect("update manager");
        let payload = b"verified update bytes";
        let mut artifact = Artifact {
            target_triple: "test".into(),
            url: format!("http://{address}/release"),
            sha256: super::hex(&Sha256::digest(payload)),
            size: payload.len() as u64,
        };
        assert_eq!(
            super::artifact(&manager.client, &artifact)
                .await
                .expect("redirected archive"),
            payload,
        );
        artifact.sha256 = "invalid".into();
        assert!(matches!(
            super::artifact(&manager.client, &artifact).await,
            Err(Error::Integrity)
        ));
        assert!(matches!(
            super::manifest(&manager.client, &format!("http://{address}/manifest")).await,
            Err(Error::Manifest)
        ));
        assert!(matches!(
            super::manifest(&manager.client, &format!("http://{address}/loop")).await,
            Err(Error::Download)
        ));
        server.abort();
    }
}
