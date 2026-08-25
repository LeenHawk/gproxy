use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FingerprintHeadersDto {
    Values(BTreeMap<String, String>),
    Disabled(bool),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum AlpnDto {
    #[serde(rename = "http/1.1")]
    #[ts(rename = "http/1.1")]
    Http1,
    #[serde(rename = "h2")]
    #[ts(rename = "h2")]
    Http2,
    #[serde(rename = "h3")]
    #[ts(rename = "h3")]
    Http3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum TlsVersionDto {
    #[serde(rename = "tls1.0")]
    #[ts(rename = "tls1.0")]
    Tls10,
    #[serde(rename = "tls1.1")]
    #[ts(rename = "tls1.1")]
    Tls11,
    #[serde(rename = "tls1.2")]
    #[ts(rename = "tls1.2")]
    Tls12,
    #[serde(rename = "tls1.3")]
    #[ts(rename = "tls1.3")]
    Tls13,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum PseudoHeaderDto {
    #[serde(rename = ":method")]
    #[ts(rename = ":method")]
    Method,
    #[serde(rename = ":scheme")]
    #[ts(rename = ":scheme")]
    Scheme,
    #[serde(rename = ":authority")]
    #[ts(rename = ":authority")]
    Authority,
    #[serde(rename = ":path")]
    #[ts(rename = ":path")]
    Path,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct TlsProfileDto {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alpn_protocols: Option<Vec<AlpnDto>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grease_enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_tls_version: Option<TlsVersionDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tls_version: Option<TlsVersionDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cipher_list: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub curves_list: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sigalgs_list: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preserve_tls13_cipher_list: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extension_permutation: Option<Vec<u16>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Http2ProfileDto {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enable_push: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initial_window_size: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initial_connection_window_size: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_frame_size: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_header_list_size: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub header_table_size: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_concurrent_streams: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub headers_pseudo_order: Option<Vec<PseudoHeaderDto>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings_order: Option<Vec<u16>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct TlsFingerprintDto {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "Record<string, string> | false")]
    pub headers: Option<FingerprintHeadersDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tls: Option<TlsProfileDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub http2: Option<Http2ProfileDto>,
}

impl TlsFingerprintDto {
    pub fn validate(&self) -> Result<(), &'static str> {
        if matches!(self.headers, Some(FingerprintHeadersDto::Disabled(true))) {
            Err("fingerprint headers boolean must be false")
        } else {
            Ok(())
        }
    }
}
