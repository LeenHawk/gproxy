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
    pub login: Option<ChannelLoginDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case")]
pub enum LoginModeDto {
    Authcode,
    Device,
    Cookie,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case")]
pub enum LoginParamKindDto {
    Text,
    Select,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct LoginParamDto {
    pub name: String,
    pub kind: LoginParamKindDto,
    pub required: bool,
    pub default_value: Option<String>,
    pub options: Vec<String>,
    pub modes: Vec<LoginModeDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ChannelLoginDto {
    pub modes: Vec<LoginModeDto>,
    pub params: Vec<LoginParamDto>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct TlsPresetDto {
    pub id: String,
    pub label: String,
    pub fingerprint: TlsFingerprintDto,
}

pub fn channel_dto(channel: &dyn gproxy_channel_api::Channel) -> ChannelDto {
    let descriptor = channel.descriptor();
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
        login: channel.login().map(|login| ChannelLoginDto {
            modes: login
                .descriptor
                .modes
                .iter()
                .copied()
                .map(login_mode)
                .collect(),
            params: login
                .descriptor
                .params
                .iter()
                .map(|param| LoginParamDto {
                    name: param.name.into(),
                    kind: match param.kind {
                        gproxy_channel_api::LoginParamKind::Text => LoginParamKindDto::Text,
                        gproxy_channel_api::LoginParamKind::Select => LoginParamKindDto::Select,
                    },
                    required: param.required,
                    default_value: param.default_value.map(Into::into),
                    options: param.options.iter().map(|value| (*value).into()).collect(),
                    modes: param.modes.iter().copied().map(login_mode).collect(),
                })
                .collect(),
        }),
    }
}

fn login_mode(mode: gproxy_channel_api::LoginMode) -> LoginModeDto {
    match mode {
        gproxy_channel_api::LoginMode::AuthCode => LoginModeDto::Authcode,
        gproxy_channel_api::LoginMode::Device => LoginModeDto::Device,
        gproxy_channel_api::LoginMode::Cookie => LoginModeDto::Cookie,
    }
}
