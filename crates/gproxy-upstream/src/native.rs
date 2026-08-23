use bytes::Bytes;
use futures_util::StreamExt;
use gproxy_channel_api::{
    BoxFuture, ByteStream, TransportError, TransportProfile, WsDuplex, WsFrame,
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
            let profile = request.extensions().get::<TransportProfile>().copied();
            let response = match profile {
                None => client.execute(request.into()).await,
                Some(TransportProfile::ClaudeCode) => {
                    let (parts, body) = request.into_parts();
                    client
                        .request(parts.method, parts.uri.to_string())
                        .headers(parts.headers)
                        .body(body)
                        .emulation(claude_code_emulation())
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
        let (parts, _) = request.into_parts();
        Box::pin(async move {
            let request = client
                .request(http::Method::GET, parts.uri.to_string())
                .headers(parts.headers);
            let response = wreq::ws::WebSocketRequestBuilder::new(request)
                .send()
                .await
                .map_err(connect_error)?;
            let socket = response.into_websocket().await.map_err(connect_error)?;
            Ok(Box::new(WreqSocket { socket }) as Box<dyn WsDuplex>)
        })
    }
}

fn claude_code_emulation() -> wreq::Emulation {
    static PROFILE: std::sync::OnceLock<wreq::Emulation> = std::sync::OnceLock::new();
    PROFILE.get_or_init(build_claude_code_emulation).clone()
}

fn build_claude_code_emulation() -> wreq::Emulation {
    use wreq::tls::{AlpnProtocol, TlsOptions, TlsVersion};

    let tls = TlsOptions::builder()
        .alpn_protocols(vec![AlpnProtocol::HTTP1])
        .grease_enabled(false)
        .min_tls_version(TlsVersion::TLS_1_2)
        .max_tls_version(TlsVersion::TLS_1_3)
        .cipher_list(
            "TLS_AES_128_GCM_SHA256:TLS_AES_256_GCM_SHA384:TLS_CHACHA20_POLY1305_SHA256:\
             ECDHE-ECDSA-AES128-GCM-SHA256:ECDHE-RSA-AES128-GCM-SHA256:\
             ECDHE-ECDSA-AES256-GCM-SHA384:ECDHE-RSA-AES256-GCM-SHA384:\
             ECDHE-ECDSA-CHACHA20-POLY1305:ECDHE-RSA-CHACHA20-POLY1305:\
             ECDHE-ECDSA-AES128-SHA:ECDHE-RSA-AES128-SHA:\
             ECDHE-ECDSA-AES256-SHA:ECDHE-RSA-AES256-SHA:\
             AES128-GCM-SHA256:AES256-GCM-SHA384:AES128-SHA:AES256-SHA"
                .to_owned(),
        )
        .curves_list("X25519:P-256:P-384".to_owned())
        .sigalgs_list(
            "ecdsa_secp256r1_sha256:rsa_pss_rsae_sha256:rsa_pkcs1_sha256:\
             ecdsa_secp384r1_sha384:rsa_pss_rsae_sha384:rsa_pkcs1_sha384:\
             rsa_pss_rsae_sha512:rsa_pkcs1_sha512:rsa_pkcs1_sha1"
                .to_owned(),
        )
        .build();
    wreq::Emulation::builder()
        .tls_options(tls)
        .build(wreq::Group::default())
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
