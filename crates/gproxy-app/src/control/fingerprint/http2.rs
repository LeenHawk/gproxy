use std::borrow::Cow;

use gproxy_channel_api::{ClientProfile, Http2Profile, Http2Setting, PseudoHeader};
use serde_json::Value;

pub(super) fn apply(value: Option<&Value>, profile: &mut ClientProfile) -> Result<(), String> {
    let Some(value) = value else {
        return Ok(());
    };
    if value == &Value::Bool(false) {
        return Ok(());
    }
    let object = value
        .as_object()
        .ok_or_else(|| "fingerprint http2 must be an object or false".to_owned())?;
    let mut output = Http2Profile {
        enable_push: optional_bool(object, "enable_push")?,
        initial_window_size: optional_u32(object, "initial_window_size")?,
        initial_connection_window_size: optional_u32(object, "initial_connection_window_size")?,
        max_frame_size: optional_u32(object, "max_frame_size")?,
        max_header_list_size: optional_u32(object, "max_header_list_size")?,
        header_table_size: optional_u32(object, "header_table_size")?,
        max_concurrent_streams: optional_u32(object, "max_concurrent_streams")?,
        ..Http2Profile::default()
    };
    if let Some(value) = object.get("headers_pseudo_order") {
        output.pseudo_header_order = Some(Cow::Owned(parse_pseudo_order(value)?));
    }
    if let Some(value) = object.get("settings_order") {
        output.settings_order = Some(Cow::Owned(parse_settings_order(value)?));
    }
    let usable = output.enable_push.is_some()
        || output.initial_window_size.is_some()
        || output.initial_connection_window_size.is_some()
        || output.max_frame_size.is_some()
        || output.max_header_list_size.is_some()
        || output.header_table_size.is_some()
        || output.max_concurrent_streams.is_some()
        || output.pseudo_header_order.is_some()
        || output.settings_order.is_some();
    if usable {
        profile.http2 = Some(output);
    }
    Ok(())
}

fn optional_bool(
    object: &serde_json::Map<String, Value>,
    field: &'static str,
) -> Result<Option<bool>, String> {
    object
        .get(field)
        .map(|value| {
            value
                .as_bool()
                .ok_or_else(|| format!("http2.{field} must be a boolean"))
        })
        .transpose()
}

fn optional_u32(
    object: &serde_json::Map<String, Value>,
    field: &'static str,
) -> Result<Option<u32>, String> {
    object
        .get(field)
        .map(|value| {
            value
                .as_u64()
                .and_then(|value| u32::try_from(value).ok())
                .ok_or_else(|| format!("http2.{field} must fit an unsigned 32-bit integer"))
        })
        .transpose()
}

fn parse_pseudo_order(value: &Value) -> Result<Vec<PseudoHeader>, String> {
    value
        .as_array()
        .ok_or("http2.headers_pseudo_order must be an array")?
        .iter()
        .map(|value| match value.as_str() {
            Some(":method") => Ok(PseudoHeader::Method),
            Some(":scheme") => Ok(PseudoHeader::Scheme),
            Some(":authority") => Ok(PseudoHeader::Authority),
            Some(":path") => Ok(PseudoHeader::Path),
            _ => Err("http2.headers_pseudo_order contains an invalid header".into()),
        })
        .collect()
}

fn parse_settings_order(value: &Value) -> Result<Vec<Http2Setting>, String> {
    value
        .as_array()
        .ok_or("http2.settings_order must be an array")?
        .iter()
        .map(|value| match value.as_u64() {
            Some(1) => Ok(Http2Setting::HeaderTableSize),
            Some(2) => Ok(Http2Setting::EnablePush),
            Some(3) => Ok(Http2Setting::MaxConcurrentStreams),
            Some(4) => Ok(Http2Setting::InitialWindowSize),
            Some(5) => Ok(Http2Setting::MaxFrameSize),
            Some(6) => Ok(Http2Setting::MaxHeaderListSize),
            _ => Err("http2.settings_order contains an invalid setting id".into()),
        })
        .collect()
}
