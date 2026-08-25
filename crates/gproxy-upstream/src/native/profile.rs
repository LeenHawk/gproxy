use gproxy_channel_api::{Alpn, ClientProfile, Http2Setting, PseudoHeader, TlsVersion};
use wreq::http2::{Http2Options, PseudoId, PseudoOrder, SettingId, SettingsOrder};
use wreq::tls::{AlpnProtocol, ExtensionType, TlsOptions, TlsVersion as WreqTlsVersion};

pub(super) fn client_emulation(profile: &ClientProfile) -> wreq::Emulation {
    let mut emulation = wreq::Emulation::builder();
    let mut tls = TlsOptions::builder();
    let mut has_tls = false;
    if let Some(alpn) = &profile.alpn {
        tls = tls.alpn_protocols(alpn.iter().map(map_alpn));
        has_tls = true;
    }
    if let Some(value) = profile.grease {
        tls = tls.grease_enabled(value);
        has_tls = true;
    }
    if let Some(value) = profile.min_tls_version {
        tls = tls.min_tls_version(map_version(value));
        has_tls = true;
    }
    if let Some(value) = profile.max_tls_version {
        tls = tls.max_tls_version(map_version(value));
        has_tls = true;
    }
    if let Some(value) = &profile.cipher_list {
        tls = tls.cipher_list(value.to_string());
        has_tls = true;
    }
    if let Some(value) = &profile.curves_list {
        tls = tls.curves_list(value.to_string());
        has_tls = true;
    }
    if let Some(value) = &profile.sigalgs_list {
        tls = tls.sigalgs_list(value.to_string());
        has_tls = true;
    }
    if let Some(value) = profile.preserve_tls13_cipher_list {
        tls = tls.preserve_tls13_cipher_list(value);
        has_tls = true;
    }
    if let Some(value) = &profile.extension_permutation {
        tls = tls.extension_permutation(std::borrow::Cow::Owned(
            value.iter().copied().map(ExtensionType::from).collect(),
        ));
        has_tls = true;
    }
    if has_tls {
        emulation = emulation.tls_options(tls.build());
    }
    if let Some(http2) = &profile.http2 {
        emulation = emulation.http2_options(http2_options(http2));
    }
    emulation.build(wreq::Group::default())
}

fn http2_options(profile: &gproxy_channel_api::Http2Profile) -> wreq::http2::Http2Options {
    let mut output = Http2Options::builder();
    if let Some(value) = profile.enable_push {
        output = output.enable_push(value);
    }
    if let Some(value) = profile.initial_window_size {
        output = output.initial_window_size(value);
    }
    if let Some(value) = profile.initial_connection_window_size {
        output = output.initial_connection_window_size(value);
    }
    if let Some(value) = profile.max_frame_size {
        output = output.max_frame_size(value);
    }
    if let Some(value) = profile.max_header_list_size {
        output = output.max_header_list_size(value);
    }
    if let Some(value) = profile.header_table_size {
        output = output.header_table_size(value);
    }
    if let Some(value) = profile.max_concurrent_streams {
        output = output.max_concurrent_streams(value);
    }
    if let Some(order) = &profile.pseudo_header_order {
        output = output.headers_pseudo_order(
            PseudoOrder::builder()
                .extend(order.iter().map(map_pseudo))
                .build(),
        );
    }
    if let Some(order) = &profile.settings_order {
        output = output.settings_order(
            SettingsOrder::builder()
                .extend(order.iter().map(map_setting))
                .build(),
        );
    }
    output.build()
}

fn map_alpn(value: &Alpn) -> AlpnProtocol {
    match value {
        Alpn::Http1 => AlpnProtocol::HTTP1,
        Alpn::Http2 => AlpnProtocol::HTTP2,
        Alpn::Http3 => AlpnProtocol::HTTP3,
    }
}

fn map_version(value: TlsVersion) -> WreqTlsVersion {
    match value {
        TlsVersion::Tls10 => WreqTlsVersion::TLS_1_0,
        TlsVersion::Tls11 => WreqTlsVersion::TLS_1_1,
        TlsVersion::Tls12 => WreqTlsVersion::TLS_1_2,
        TlsVersion::Tls13 => WreqTlsVersion::TLS_1_3,
    }
}

fn map_pseudo(value: &PseudoHeader) -> PseudoId {
    match value {
        PseudoHeader::Method => PseudoId::Method,
        PseudoHeader::Scheme => PseudoId::Scheme,
        PseudoHeader::Authority => PseudoId::Authority,
        PseudoHeader::Path => PseudoId::Path,
    }
}

fn map_setting(value: &Http2Setting) -> SettingId {
    match value {
        Http2Setting::HeaderTableSize => SettingId::HeaderTableSize,
        Http2Setting::EnablePush => SettingId::EnablePush,
        Http2Setting::MaxConcurrentStreams => SettingId::MaxConcurrentStreams,
        Http2Setting::InitialWindowSize => SettingId::InitialWindowSize,
        Http2Setting::MaxFrameSize => SettingId::MaxFrameSize,
        Http2Setting::MaxHeaderListSize => SettingId::MaxHeaderListSize,
    }
}
