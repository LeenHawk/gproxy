use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::TlsFingerprintDto;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ChannelSupportDto {
    pub source: String,
    pub target: String,
    pub operation: String,
    pub group: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ChannelDto {
    pub id: String,
    pub display_name: String,
    pub supports: Vec<ChannelSupportDto>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct TlsPresetDto {
    pub id: String,
    pub label: String,
    pub fingerprint: TlsFingerprintDto,
}

pub fn channel_dto(descriptor: &gproxy_channel_api::ChannelDescriptor) -> ChannelDto {
    ChannelDto {
        id: descriptor.id.into(),
        display_name: descriptor.display_name.into(),
        supports: descriptor
            .supports
            .iter()
            .map(|support| ChannelSupportDto {
                source: support.source.kind.id().into(),
                target: support.target.kind.id().into(),
                operation: support.source.operation.id().into(),
                group: support.source.operation.group().id().into(),
            })
            .collect(),
    }
}
