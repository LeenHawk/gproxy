#![cfg(all(feature = "s3", not(target_arch = "wasm32")))]

#[path = "support/s3.rs"]
mod storage;

#[cfg(any(feature = "openai", feature = "gemini"))]
mod support;

use gproxy_client_sdk::{s3::*, *};

use std::{sync::atomic::Ordering, time::Duration};

fn data(mime: &str) -> AssetData {
    AssetData {
        bytes: b"abc".to_vec(),
        media_type: mime.into(),
    }
}

fn input(target: AssetDialect, kind: MediaKind, mime: Option<&str>) -> ResolveAsset {
    let capability = media_capability(target, kind);
    ResolveAsset {
        reference: "hash-of-app-blob".into(),
        target,
        kind,
        location: AssetLocation::Message,
        media_type: mime.map(str::to_owned),
        preferred: capability.preferred(DeliveryMode::PreferUrl).unwrap(),
        capability,
    }
}

#[path = "s3/client.rs"]
mod client;

#[path = "s3/policy.rs"]
mod policy;

#[path = "s3/objects.rs"]
mod objects;
