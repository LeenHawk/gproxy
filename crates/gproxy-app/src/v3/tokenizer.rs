//! v3's tokenizer vocabularies and the token they were downloaded with.
//!
//! Database route only: v3's export carried neither. v3 kept each
//! vocabulary's bytes in `tokenizer_vocabs` by name and chose the instance
//! default by that name (`default_tokenizer_vocab`); v4 keeps them as stored
//! files and chooses the default by file id. Each vocabulary is stored
//! through the sdk's own `tokenizer().import`, the path a downloaded one
//! takes, and the one v3 named as the default becomes v4's.
//!
//! The Hugging Face token was sealed with v3's credential envelope; it is
//! opened with the source key and handed to the sdk, which seals it with
//! this instance's.

use std::sync::Arc;

use crate::App;
use bytes::Bytes;
use gproxy_seaorm::BatchConnectionTrait;
use serde_json::Value;

use super::Result;
use super::{
    Report,
    document::Document,
    secret::{Bridge, Domain},
};

pub async fn write<C>(
    app: &Arc<App<C>>,
    document: &Document,
    bridge: &Bridge,
    report: &mut Report,
) -> Result<()>
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    let data = &document.data;
    let tokenizer = app.gproxy().manage().tokenizer();
    if let Some(envelope) = &data.tokenizer_auth {
        match bridge.open(Domain::Credential, "tokenizer auth", envelope)? {
            Value::String(token) if !token.trim().is_empty() => {
                tokenizer.set_auth(Some(token)).await?;
                report.count("tokenizer_auth", 1);
            }
            _ => report.drop_row(
                "tokenizer_auth",
                "hugging_face".to_owned(),
                "the sealed value is not a token",
            ),
        }
    }

    let default = data
        .settings
        .get("default_tokenizer_vocab")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty());
    let mut stored = 0;
    for vocab in &data.tokenizer_vocabs {
        let is_default = default == Some(vocab.name.as_str());
        match tokenizer
            .import(
                format!("{}.json", vocab.name),
                Bytes::from(vocab.bytes.clone()),
                is_default,
            )
            .await
        {
            Ok(_) => stored += 1,
            Err(error) => report.drop_row(
                "tokenizer_vocabs",
                vocab.name.clone(),
                format!("it could not be stored: {error}"),
            ),
        }
    }
    report.count("vocabularies", stored);
    if let Some(name) = default
        && !data.tokenizer_vocabs.iter().any(|vocab| vocab.name == name)
    {
        report.warn(format!(
            "setting default_tokenizer_vocab names `{name}`, which the v3 database does not \
             hold; the destination keeps its own default"
        ));
    }
    Ok(())
}
