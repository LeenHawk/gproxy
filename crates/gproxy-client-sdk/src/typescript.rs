use crate::*;
/// Export the complete public IR graph using the workspace's ts-rs setup.
/// The caller chooses the output directory; normal builds write nothing.
#[cfg(feature = "ts")]
pub fn export_types(
    directory: impl Into<std::path::PathBuf>,
) -> std::result::Result<(), ts_rs::ExportError> {
    use ts_rs::TS;
    let config = ts_rs::Config::new()
        .with_out_dir(directory)
        .with_large_int("number");
    AssetDialect::export_all(&config)?;
    MediaCapability::export_all(&config)?;
    #[cfg(any(
        feature = "claude",
        feature = "openai",
        feature = "chat",
        feature = "gemini",
        feature = "embeddings",
        feature = "s3"
    ))]
    {
        ResolveAsset::export_all(&config)?;
        ResolvedAsset::export_all(&config)?;
    }
    #[cfg(feature = "s3")]
    s3::AssetData::export_all(&config)?;
    Conversation::export_all(&config)?;
    GenerateRequest::export_all(&config)?;
    GenerateResult::export_all(&config)?;
    Event::export_all(&config)?;
    #[cfg(feature = "embeddings")]
    {
        embeddings::EmbedRequest::export_all(&config)?;
        embeddings::EmbedResult::export_all(&config)?;
    }
    operations::Params::<ts_rs::Dummy>::export_all(&config)?;
    #[cfg(feature = "count-tokens")]
    operations::TokenCount::export_all(&config)?;
    #[cfg(feature = "audio")]
    operations::TranscriptionParams::export_all(&config)?;
    Ok(())
}
