use bytes::Bytes;
use futures_util::StreamExt;
use gproxy_channel_api::{
    Alpn, BoxFuture, ByteStream, ClientProfile, Http2Setting, PseudoHeader, TlsVersion,
    TransportError, WsDuplex, WsFrame,
};
use gproxy_core::UpstreamTransport;

#[derive(Clone)]
pub struct WreqTransport {
    client: wreq::Client,
}

impl WreqTransport {
    pub fn new() -> Self {
        Self {
            client: wreq::Client::builder()
                .user_agent(concat!("gproxy/", env!("CARGO_PKG_VERSION")))
                .build()
                .expect("default wreq client builds"),
        }
    }

    pub fn from_client(client: wreq::Client) -> Self {
        Self { client }
    }
}

impl Default for WreqTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl UpstreamTransport for WreqTransport {
    fn send<'a>(
        &'a self,
        request: http::Request<Bytes>,
    ) -> BoxFuture<'a, Result<http::Response<ByteStream>, TransportError>> {
        let client = self.client.clone();
        Box::pin(async move {
            let profile = request.extensions().get::<ClientProfile>().copied();
            let response = match profile {
                None => client.execute(request.into()).await,
                Some(profile) => {
                    let (parts, body) = request.into_parts();
                    client
                        .request(parts.method, parts.uri.to_string())
                        .headers(parts.headers)
                        .body(body)
                        .emulation(client_emulation(profile))
                        .send()
                        .await
                }
            }
            .map_err(connect_error)?;
            let status = response.status();
            let version = response.version();
            let headers = response.headers().clone();
            let stream: ByteStream = Box::pin(
                response
                    .bytes_stream()
                    .map(|chunk| chunk.map_err(interrupted_error)),
            );
            let mut response = http::Response::new(stream);
            *response.status_mut() = status;
            *response.version_mut() = version;
            *response.headers_mut() = headers;
            Ok(response)
        })
    }

    fn open_websocket<'a>(
        &'a self,
        request: http::Request<Bytes>,
    ) -> BoxFuture<'a, Result<Box<dyn WsDuplex>, TransportError>> {
        let client = self.client.clone();
        let profile = request.extensions().get::<ClientProfile>().copied();
        let (parts, _) = request.into_parts();
        Box::pin(async move {
            let mut request = client
                .request(http::Method::GET, parts.uri.to_string())
                .headers(parts.headers);
            request = match profile {
                None => request,
                Some(profile) => request.emulation(client_emulation(profile)),
            };
            let response = wreq::ws::WebSocketRequestBuilder::new(request)
                .send()
                .await
                .map_err(connect_error)?;
            let socket = response.into_websocket().await.map_err(connect_error)?;
            Ok(Box::new(WreqSocket { socket }) as Box<dyn WsDuplex>)
        })
    }
}

fn client_emulation(profile: ClientProfile) -> wreq::Emulation {
    use wreq::http2::{Http2Options, PseudoId, PseudoOrder, SettingId, SettingsOrder};
    use wreq::tls::{AlpnProtocol, TlsOptions, TlsVersion as WreqTlsVersion};

    let alpn: Vec<_> = profile
        .alpn
        .iter()
        .map(|protocol| match protocol {
            Alpn::Http1 => AlpnProtocol::HTTP1,
            Alpn::Http2 => AlpnProtocol::HTTP2,
        })
        .collect();
    let version = |version| match version {
        TlsVersion::Tls12 => WreqTlsVersion::TLS_1_2,
        TlsVersion::Tls13 => WreqTlsVersion::TLS_1_3,
    };
    let mut tls = TlsOptions::builder()
        .alpn_protocols(alpn)
        .grease_enabled(profile.grease)
        .min_tls_version(version(profile.min_tls_version))
        .max_tls_version(version(profile.max_tls_version))
        .cipher_list(profile.cipher_list.to_owned())
        .curves_list(profile.curves_list.to_owned());
    if let Some(sigalgs) = profile.sigalgs_list {
        tls = tls.sigalgs_list(sigalgs.to_owned());
    }
    let mut emulation = wreq::Emulation::builder().tls_options(tls.build());
    if let Some(profile) = profile.http2 {
        let pseudo_order = profile
            .pseudo_header_order
            .iter()
            .map(|header| match header {
                PseudoHeader::Method => PseudoId::Method,
                PseudoHeader::Scheme => PseudoId::Scheme,
                PseudoHeader::Authority => PseudoId::Authority,
                PseudoHeader::Path => PseudoId::Path,
            });
        let settings_order = profile.settings_order.iter().map(|setting| match setting {
            Http2Setting::EnablePush => SettingId::EnablePush,
            Http2Setting::InitialWindowSize => SettingId::InitialWindowSize,
            Http2Setting::MaxFrameSize => SettingId::MaxFrameSize,
            Http2Setting::MaxHeaderListSize => SettingId::MaxHeaderListSize,
        });
        let http2 = Http2Options::builder()
            .enable_push(profile.enable_push)
            .initial_window_size(profile.initial_window_size)
            .initial_connection_window_size(profile.initial_connection_window_size)
            .max_frame_size(profile.max_frame_size)
            .max_header_list_size(profile.max_header_list_size)
            .headers_pseudo_order(PseudoOrder::builder().extend(pseudo_order).build())
            .settings_order(SettingsOrder::builder().extend(settings_order).build())
            .build();
        emulation = emulation.http2_options(http2);
    }
    emulation.build(wreq::Group::default())
}

struct WreqSocket {
    socket: wreq::ws::WebSocket,
}

impl WsDuplex for WreqSocket {
    fn send<'a>(&'a mut self, frame: WsFrame) -> BoxFuture<'a, Result<(), TransportError>> {
        Box::pin(async move {
            use wreq::ws::message::{CloseFrame, Message};

            let message = match frame {
                WsFrame::Text(text) => Message::text(text),
                WsFrame::Binary(bytes) => Message::binary(bytes),
                WsFrame::Close(code) => Message::close(code.map(|code| CloseFrame {
                    code: code.into(),
                    reason: "".into(),
                })),
            };
            self.socket.send(message).await.map_err(interrupted_error)
        })
    }

    fn recv<'a>(&'a mut self) -> BoxFuture<'a, Result<Option<WsFrame>, TransportError>> {
        Box::pin(async move {
            use wreq::ws::message::Message;

            loop {
                let Some(message) = self.socket.recv().await else {
                    return Ok(None);
                };
                match message.map_err(interrupted_error)? {
                    Message::Text(text) => {
                        return Ok(Some(WsFrame::Text(text.as_str().to_owned())));
                    }
                    Message::Binary(bytes) => return Ok(Some(WsFrame::Binary(bytes))),
                    Message::Close(frame) => {
                        return Ok(Some(WsFrame::Close(
                            frame.map(|frame| u16::from(frame.code)),
                        )));
                    }
                    Message::Ping(_) | Message::Pong(_) => {}
                }
            }
        })
    }
}

fn connect_error(error: wreq::Error) -> TransportError {
    map_error(error, TransportError::Connect)
}

fn interrupted_error(error: wreq::Error) -> TransportError {
    map_error(error, TransportError::Interrupted)
}

fn map_error(error: wreq::Error, other: fn(String) -> TransportError) -> TransportError {
    if error.is_timeout() {
        TransportError::Timeout
    } else {
        other(error.without_uri().to_string())
    }
}
