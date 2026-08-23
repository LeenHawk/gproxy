macro_rules! extensible_string_enum {
    ($outer:ident, $known:ident { $($variant:ident => $wire:literal),+ $(,)? }) => {
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub enum $outer {
            Known($known),
            Unknown(String),
        }

        impl $outer {
            pub fn as_str(&self) -> &str {
                match self {
                    Self::Known(known) => known.as_str(),
                    Self::Unknown(value) => value,
                }
            }
        }

        impl serde::Serialize for $outer {
            fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_str(self.as_str())
            }
        }

        impl<'de> serde::Deserialize<'de> for $outer {
            fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let value = <String as serde::Deserialize>::deserialize(deserializer)?;
                Ok(match value.as_str() {
                    $($wire => Self::Known($known::$variant),)+
                    _ => Self::Unknown(value),
                })
            }
        }

        #[derive(Debug, Clone, PartialEq, Eq)]
        pub enum $known {
            $($variant,)+
        }

        impl $known {
            pub fn as_str(&self) -> &'static str {
                match self {
                    $(Self::$variant => $wire,)+
                }
            }
        }

        impl serde::Serialize for $known {
            fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_str(self.as_str())
            }
        }

        impl<'de> serde::Deserialize<'de> for $known {
            fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let value = <String as serde::Deserialize>::deserialize(deserializer)?;
                match value.as_str() {
                    $($wire => Ok(Self::$variant),)+
                    _ => Err(serde::de::Error::unknown_variant(&value, &[$($wire,)+])),
                }
            }
        }
    };
}

macro_rules! strict_string_enum {
    ($outer:ident { $($variant:ident => $wire:literal),+ $(,)? }) => {
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub enum $outer {
            $($variant,)+
            Unknown(String),
        }

        impl $outer {
            pub fn as_str(&self) -> &str {
                match self {
                    $(Self::$variant => $wire,)+
                    Self::Unknown(value) => value,
                }
            }
        }

        impl serde::Serialize for $outer {
            fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_str(self.as_str())
            }
        }

        impl<'de> serde::Deserialize<'de> for $outer {
            fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let value = <String as serde::Deserialize>::deserialize(deserializer)?;
                Ok(match value.as_str() {
                    $($wire => Self::$variant,)+
                    _ => Self::Unknown(value),
                })
            }
        }
    };
}

mod enums;
mod formats;
mod logprobs;
mod model_ids;
mod object_types;
mod tools;
mod types;

pub use enums::*;
pub use formats::*;
pub use logprobs::*;
pub use model_ids::*;
pub use object_types::*;
pub use tools::*;
pub use types::*;
