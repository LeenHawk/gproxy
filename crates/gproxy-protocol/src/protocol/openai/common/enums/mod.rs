macro_rules! extensible_string_enum {
    ($outer:ident, $known:ident { $($variant:ident => $wire:literal),+ $(,)? }) => {
        #[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
        #[serde(untagged)]
        pub enum $outer {
            Known($known),
            Unknown(String),
        }

        // Manual Deserialize: a known-variant miss falls back to `Unknown`
        // without ever formatting an unknown-variant error (see
        // `protocol::extensible`).
        impl<'de> serde::Deserialize<'de> for $outer {
            fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                crate::protocol::extensible::deserialize_extensible(d, Self::Known, Self::Unknown)
            }
        }

        impl $outer {
            /// Wire string of this value (known rename or the raw unknown).
            pub fn as_str(&self) -> &str {
                match self {
                    Self::Known(known) => known.as_str(),
                    Self::Unknown(other) => other,
                }
            }
        }

        #[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
        pub enum $known {
            $(
                #[serde(rename = $wire)]
                $variant,
            )+
        }

        impl $known {
            /// Wire string of this variant (identical to the serde rename).
            pub fn as_str(&self) -> &'static str {
                match self {
                    $(Self::$variant => $wire,)+
                }
            }
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
