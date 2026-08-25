use std::sync::Arc;

use super::{
    BUNDLED_NAMES, BUNDLED_PRIMARY, LoadRequestStatus, TokenizerRegistry, VocabSource, info,
};

impl TokenizerRegistry {
    pub async fn list(&self) -> Vec<super::VocabInfo> {
        let mut entries = Vec::new();
        #[cfg(feature = "tiktoken")]
        entries.extend([
            info("o200k_base", VocabSource::BuiltinTiktoken, true),
            info("cl100k_base", VocabSource::BuiltinTiktoken, true),
        ]);
        entries.push(info(
            BUNDLED_PRIMARY,
            VocabSource::Bundled,
            self.loaded.contains_key(BUNDLED_PRIMARY),
        ));
        match self.store.list().await {
            Ok(names) => entries.extend(names.into_iter().map(|name| {
                let loaded = self.loaded.contains_key(&name);
                info(&name, VocabSource::Downloaded, loaded)
            })),
            Err(error) => tracing::warn!(%error, "listing tokenizer vocabularies failed"),
        }
        entries
    }

    pub fn resolve(&self, name: &str) -> Option<Arc<tokenizers::Tokenizer>> {
        if let Some(tokenizer) = self.loaded.get(name) {
            return Some(Arc::clone(&tokenizer));
        }
        if BUNDLED_NAMES.contains(&name) {
            let tokenizer = super::asset::tokenizer()?;
            for alias in BUNDLED_NAMES {
                self.loaded
                    .insert((*alias).to_owned(), Arc::clone(&tokenizer));
            }
            return Some(tokenizer);
        }
        None
    }

    pub fn preheat(&self) -> LoadRequestStatus {
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            return LoadRequestStatus::NoRuntime;
        };
        let loaded = Arc::clone(&self.loaded);
        runtime.spawn_blocking(move || {
            if let Some(tokenizer) = super::asset::tokenizer() {
                for alias in BUNDLED_NAMES {
                    loaded.insert((*alias).to_owned(), Arc::clone(&tokenizer));
                }
            }
        });
        LoadRequestStatus::Scheduled
    }
}
