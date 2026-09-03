use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::RoutingImplementationDto;
use super::TlsFingerprintDto;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ChannelSupportDto {
    pub source: String,
    pub target: String,
    pub operation: String,
    pub target_operation: String,
    pub group: String,
    pub implementation: RoutingImplementationDto,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct ChannelDto {
    pub id: String,
    pub display_name: String,
    pub supports: Vec<ChannelSupportDto>,
    pub routing_defaults: Vec<ChannelSupportDto>,
    pub login: Option<ChannelLoginDto>,
    pub provider_fields: Vec<ChannelFieldDto>,
    pub credential_fields: Vec<ChannelFieldDto>,
    pub endpoint_kinds: Vec<String>,
    pub traffic_policy: super::TrafficPolicyDto,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case")]
pub enum ChannelFieldControlDto {
    Text,
    Secret,
    Url,
    Integer,
    Boolean,
    StringList,
    Select,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ChannelFieldDto {
    pub key: String,
    pub i18n_key: String,
    pub control: ChannelFieldControlDto,
    pub required: bool,
    pub advanced: bool,
    pub default_value: Option<String>,
    pub options: Vec<String>,
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
pub struct LoginParamConditionDto {
    pub param: String,
    pub equals: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct LoginParamDto {
    pub name: String,
    pub kind: LoginParamKindDto,
    pub required: bool,
    pub default_value: Option<String>,
    pub options: Vec<String>,
    pub modes: Vec<LoginModeDto>,
    pub condition: Option<LoginParamConditionDto>,
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
        supports: descriptor.supports.iter().map(channel_support).collect(),
        routing_defaults: channel
            .routing_table()
            .iter()
            .map(channel_support)
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
                    condition: param
                        .condition
                        .as_ref()
                        .map(|condition| LoginParamConditionDto {
                            param: condition.param.into(),
                            equals: condition.equals.into(),
                        }),
                })
                .collect(),
        }),
        // Not channel knowledge: any provider that can list models can be told to
        // stop asking upstream on every request, so it is added once here instead
        // of repeated in twenty-eight declarations.
        provider_fields: std::iter::once(ChannelFieldDto {
            key: "auto_refresh_models".into(),
            i18n_key: "auto_refresh_models".into(),
            control: ChannelFieldControlDto::Boolean,
            required: false,
            advanced: true,
            default_value: Some("true".into()),
            options: Vec::new(),
        })
        .chain(descriptor.provider_fields.iter().map(channel_field))
        .collect(),
        credential_fields: descriptor
            .credential_fields
            .iter()
            .map(channel_field)
            .collect(),
        endpoint_kinds: if descriptor.endpoint_overrides {
            descriptor
                .supports
                .iter()
                .filter_map(|support| gproxy_channel_api::endpoint_override_key(support.target))
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .map(Into::into)
                .collect()
        } else {
            Vec::new()
        },
        traffic_policy: descriptor.traffic_policy.into(),
    }
}

fn channel_support(support: &gproxy_channel_api::ChannelSupport) -> ChannelSupportDto {
    ChannelSupportDto {
        source: support.source.kind().id().into(),
        target: support.target.kind().id().into(),
        operation: support.source.operation().id().into(),
        target_operation: support.target.operation().id().into(),
        group: support.source.operation().group().id().into(),
        implementation: match support.action {
            gproxy_channel_api::ChannelRouteAction::Passthrough => {
                RoutingImplementationDto::Passthrough
            }
            gproxy_channel_api::ChannelRouteAction::TransformTo => {
                RoutingImplementationDto::TransformTo
            }
            gproxy_channel_api::ChannelRouteAction::Local => RoutingImplementationDto::Local,
            gproxy_channel_api::ChannelRouteAction::Unsupported => {
                RoutingImplementationDto::Unsupported
            }
        },
    }
}

fn channel_field(field: &gproxy_channel_api::ChannelField) -> ChannelFieldDto {
    ChannelFieldDto {
        key: field.key.into(),
        i18n_key: field.i18n_key.into(),
        control: match field.control {
            gproxy_channel_api::ChannelFieldControl::Text => ChannelFieldControlDto::Text,
            gproxy_channel_api::ChannelFieldControl::Secret => ChannelFieldControlDto::Secret,
            gproxy_channel_api::ChannelFieldControl::Url => ChannelFieldControlDto::Url,
            gproxy_channel_api::ChannelFieldControl::Integer => ChannelFieldControlDto::Integer,
            gproxy_channel_api::ChannelFieldControl::Boolean => ChannelFieldControlDto::Boolean,
            gproxy_channel_api::ChannelFieldControl::StringList => {
                ChannelFieldControlDto::StringList
            }
            gproxy_channel_api::ChannelFieldControl::Select => ChannelFieldControlDto::Select,
        },
        required: field.required,
        advanced: field.advanced,
        default_value: field.default_value.map(Into::into),
        options: field.options.iter().map(|value| (*value).into()).collect(),
    }
}

fn login_mode(mode: gproxy_channel_api::LoginMode) -> LoginModeDto {
    match mode {
        gproxy_channel_api::LoginMode::AuthCode => LoginModeDto::Authcode,
        gproxy_channel_api::LoginMode::Device => LoginModeDto::Device,
        gproxy_channel_api::LoginMode::Cookie => LoginModeDto::Cookie,
    }
}
