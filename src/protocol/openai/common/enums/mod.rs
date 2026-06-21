macro_rules! extensible_string_enum {
    ($outer:ident, $known:ident { $($variant:ident => $wire:literal),+ $(,)? }) => {
        #[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
        #[serde(untagged)]
        pub enum $outer {
            Known($known),
            Unknown(String),
        }

        #[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
        pub enum $known {
            $(
                #[serde(rename = $wire)]
                $variant,
            )+
        }
    };
}

macro_rules! strict_string_enum {
    ($outer:ident { $($variant:ident => $wire:literal),+ $(,)? }) => {
        #[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
        pub enum $outer {
            $(
                #[serde(rename = $wire)]
                $variant,
            )+
        }
    };
}

mod chat;
mod content;
mod images;
mod responses;
mod tools;

pub use chat::*;
pub use content::*;
pub use images::*;
pub use responses::*;
pub use tools::*;
