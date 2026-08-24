mod request;
mod response;
mod value;

use serde::{Deserialize, Serialize};

pub(super) use request::{encode_batch, encode_execute};
pub(super) use response::{decode_batch, decode_execute};

#[derive(Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum WireValue {
    Null,
    Integer { value: String },
    Float { value: f64 },
    Text { value: String },
    Blob { base64: String },
}
