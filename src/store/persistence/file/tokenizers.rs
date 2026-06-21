//! File-backend tokenizer vocab store: one raw `tokenizer.json` blob per
//! vocab under `{root}/tokenizers/{sanitized}.json`. These are multi-MB
//! files, so they bypass the JSON-table machinery; writes are atomic
//! (temp file + rename).

use std::path::{Path, PathBuf};

fn dir(root: &Path) -> PathBuf {
    root.join("tokenizers")
}

/// HF repo paths contain `/`; keep disk names flat.
fn sanitize(name: &str) -> String {
    name.replace('/', "--")
}

fn vocab_path(root: &Path, name: &str) -> PathBuf {
    dir(root).join(format!("{}.json", sanitize(name)))
}

pub(crate) async fn list(root: &Path) -> anyhow::Result<Vec<String>> {
    let mut out = Vec::new();
    let mut entries = match tokio::fs::read_dir(dir(root)).await {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(out),
        Err(e) => return Err(e.into()),
    };
    while let Some(entry) = entries.next_entry().await? {
        let file = entry.file_name().to_string_lossy().into_owned();
        if let Some(stem) = file.strip_suffix(".json") {
            out.push(stem.replace("--", "/"));
        }
    }
    Ok(out)
}

pub(crate) async fn get(root: &Path, name: &str) -> anyhow::Result<Option<Vec<u8>>> {
    match tokio::fs::read(vocab_path(root, name)).await {
        Ok(bytes) => Ok(Some(bytes)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

pub(crate) async fn put(root: &Path, name: &str, bytes: &[u8]) -> anyhow::Result<()> {
    tokio::fs::create_dir_all(dir(root)).await?;
    let path = vocab_path(root, name);
    let tmp = path.with_extension("json.tmp");
    tokio::fs::write(&tmp, bytes).await?;
    tokio::fs::rename(&tmp, &path).await?;
    Ok(())
}
