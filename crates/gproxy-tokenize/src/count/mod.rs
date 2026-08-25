mod strict;
mod sync;

pub use strict::try_count;
pub use sync::{count, count_detailed};

use crate::{CountResult, CountWarning};

pub(crate) const MESSAGE_OVERHEAD: u64 = 4;

#[cfg(feature = "hf-registry")]
pub type RegistryHandle<'a> = &'a crate::TokenizerRegistry;
#[cfg(not(feature = "hf-registry"))]
pub type RegistryHandle<'a> = ();

pub fn count_text(text: &str) -> u64 {
    #[cfg(feature = "tiktoken")]
    {
        tiktoken_rs::cl100k_base_singleton()
            .encode_ordinary(text)
            .len() as u64
    }
    #[cfg(not(feature = "tiktoken"))]
    {
        char_count(text).div_ceil(2)
    }
}

pub(crate) struct Prepared {
    pub joined: String,
    pub messages: u64,
    pub warnings: Vec<CountWarning>,
}

impl Prepared {
    pub fn lossy(body: &[u8]) -> Self {
        match crate::try_harvest(body) {
            Ok((texts, messages)) => Self::from_harvest(texts, messages),
            Err(error) => Self {
                joined: String::from_utf8_lossy(body).into_owned(),
                messages: 0,
                warnings: vec![
                    CountWarning::InvalidJson {
                        reason: error.to_string(),
                    },
                    CountWarning::RawBodyEstimate,
                ],
            },
        }
    }

    pub fn from_harvest(texts: Vec<String>, messages: u64) -> Self {
        Self {
            joined: texts.join("\n"),
            messages,
            warnings: vec![
                CountWarning::ApproximateProviderFraming {
                    tokens_per_message: MESSAGE_OVERHEAD,
                },
                CountWarning::GenericJsonHarvest,
            ],
        }
    }

    pub fn character_result(self) -> CountResult {
        CountResult {
            tokens: char_count(&self.joined).div_ceil(2)
                + self.messages.saturating_mul(MESSAGE_OVERHEAD),
            method: crate::CountMethod::CharacterEstimate,
            vocab: None,
            warnings: self.warnings,
        }
    }
}

fn char_count(text: &str) -> u64 {
    text.chars().count() as u64
}
