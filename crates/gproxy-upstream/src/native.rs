use bytes::Bytes;
use futures_util::StreamExt;
use gproxy_channel_api::{BoxFuture, ByteStream, ClientProfile, TransportError, WsDuplex, WsFrame};
use gproxy_core::{UpstreamProxy, UpstreamTransport};

mod profile;

#[derive(Clone)]
pub struct WreqTransport {
    direct_client: wreq::Client,
    system_client: wreq::Client,
    inherit_system_proxy: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl WreqTransport {
    pub fn new() -> Self {
        Self::with_system_proxy(false)
    }

    pub fn with_system_proxy(inherit_system_proxy: bool) -> Self {
        let builder =
            || wreq::Client::builder().user_agent(concat!("gproxy/", env!("CARGO_PKG_VERSION")));
        Self {
            direct_client: builder()
                .no_proxy()
                .build()
                .expect("direct wreq client builds"),
            system_client: builder().build().expect("system-proxy wreq client builds"),
            inherit_system_proxy: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(
                inherit_system_proxy,
            )),
        }
    }

    pub fn from_client(client: wreq::Client) -> Self {
        Self {
            direct_client: client.clone(),
            system_client: client,
            inherit_system_proxy: Default::default(),
        }
    }

    pub fn set_inherit_system_proxy(&self, inherit: bool) {
        self.inherit_system_proxy
            .store(inherit, std::sync::atomic::Ordering::Relaxed);
    }

    fn client(&self) -> wreq::Client {
        if self
            .inherit_system_proxy
            .load(std::sync::atomic::Ordering::Relaxed)
        {
            self.system_client.clone()
        } else {
            self.direct_client.clone()
        }
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
        let client = self.client();
        Box::pin(async move {
            let profile = request.extensions().get::<ClientProfile>().cloned();
            let proxy = request.extensions().get::<UpstreamProxy>().cloned();
            let (parts, body) = request.into_parts();
            let mut request = client
                .request(parts.method, parts.uri.to_string())
                .headers(parts.headers)
                .body(body);
            if let Some(profile) = profile {
                request = request.emulation(profile::client_emulation(&profile));
            }
            if let Some(proxy) = proxy {
                request = request.proxy(wreq::Proxy::all(&proxy.0).map_err(connect_error)?);
            }
            let response = request.send().await.map_err(connect_error)?;
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
        let client = self.client();
        let profile = request.extensions().get::<ClientProfile>().cloned();
        let proxy = request.extensions().get::<UpstreamProxy>().cloned();
        let (parts, _) = request.into_parts();
        Box::pin(async move {
            let mut request = client
                .request(http::Method::GET, parts.uri.to_string())
                .headers(parts.headers);
            request = match profile {
                None => request,
                Some(profile) => request.emulation(profile::client_emulation(&profile)),
            };
            let mut request = wreq::ws::WebSocketRequestBuilder::new(request);
            if let Some(proxy) = proxy {
                request = request.proxy(wreq::Proxy::all(&proxy.0).map_err(connect_error)?);
            }
            let response = request.send().await.map_err(connect_error)?;
            let socket = response.into_websocket().await.map_err(connect_error)?;
            Ok(Box::new(WreqSocket { socket }) as Box<dyn WsDuplex>)
        })
    }
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
    } else if let Some(status) = error.status() {
        TransportError::Status(status.as_u16())
    } else {
        other(error.without_uri().to_string())
    }
}
