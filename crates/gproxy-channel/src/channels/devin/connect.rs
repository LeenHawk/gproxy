//! Connect-RPC framing: the envelope this channel writes, the multi-frame
//! stream it reads back, and the headers both carry.
//!
//! An envelope is `[1 byte flags][4 byte big-endian length][payload]`, with
//! flag `0x01` for a gzip-compressed payload and `0x02` for the end-of-stream
//! frame whose payload is a JSON trailer (`{}` on success, `{"error":{…}}`
//! otherwise). Source: `samples/windsurfapi/src/connect.js`, whose parser was
//! calibrated against live captures of this same upstream.
//!
//! The request envelope goes out **uncompressed**: the reference records that
//! a gzipped request frame is rejected with an opaque "internal" error while
//! the server still streams gzipped frames back
//! (`samples/windsurfapi/src/devin-connect.js`, the comment above
//! `wrapEnvelope(proto, { compress: false })`). That asymmetry is why this
//! module inflates but never deflates.

use http::{HeaderMap, HeaderName, HeaderValue, header};

use crate::channel::ChannelError;

/// The origin every call in this channel goes to. A live capture of the real
/// teams CLI sends `GetChatMessage` here rather than to an account-specific
/// `apiServerUrl` (`devin-connect.js`, the host-resolution comment that
/// reverts the per-account host experiment).
pub const DEFAULT_BASE_URL: &str = "https://server.codeium.com";
/// The generation method. Source: `devin-connect.js` file header.
pub const CHAT_PATH: &str = "/exa.api_server_pb.ApiServerService/GetChatMessage";
/// Account plan and quota. Source: `samples/windsurfapi/src/windsurf-api.js`
/// and `samples/cpa-manager-plus/apps/web/src/utils/quota/constants.ts`, which
/// agree on the path and on the JSON codec.
pub const USER_STATUS_PATH: &str = "/exa.seat_management_pb.SeatManagementService/GetUserStatus";
/// The optional short-lived user JWT mint. Not called by this channel; see the
/// module documentation for why the credential carries the JWT instead.
pub const USER_JWT_PATH: &str = "/exa.auth_pb.AuthService/GetUserJwt";

/// `application/connect+proto`, `connect-protocol-version: 1`,
/// `connect-accept-encoding: gzip`, `user-agent: connect-es/2.0.0`
/// (`devin-connect.js` file header and `connect.js::connectHeaders`).
pub const PROTO_CONTENT_TYPE: &str = "application/connect+proto";
pub const PROTOCOL_VERSION: &str = "1";
pub const ACCEPT_ENCODING: &str = "gzip";
pub const USER_AGENT: &str = "connect-es/2.0.0";

/// Ceiling on one frame, for both its advertised length and the size it may
/// inflate to. The reference bounds both at the same value for the same
/// reason: a high-ratio frame from a hostile upstream must not be able to
/// exhaust memory before the error is caught.
pub const MAX_FRAME_BYTES: usize = 16 * 1024 * 1024;

const FLAG_COMPRESSED: u8 = 0x01;
const FLAG_END_STREAM: u8 = 0x02;

/// One decoded envelope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    /// The end-of-stream frame: its payload is the JSON trailer, not a message.
    pub end_stream: bool,
    /// Payload after any frame-level decompression.
    pub payload: Vec<u8>,
}

/// Wrap one protobuf message as the single request envelope. Uncompressed by
/// deliberate calibration, see the module documentation.
pub fn request_frame(payload: &[u8]) -> Vec<u8> {
    let mut frame = Vec::with_capacity(payload.len() + 5);
    frame.push(0);
    frame.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    frame.extend_from_slice(payload);
    frame
}

/// Wrap a payload with explicit flags. Only tests and the trailer need this;
/// the request path calls [`request_frame`].
pub fn frame_with_flags(flags: u8, payload: &[u8]) -> Vec<u8> {
    let mut frame = request_frame(payload);
    frame[0] = flags;
    frame
}

/// The end-of-stream trailer a successful Connect stream ends with.
pub fn end_of_stream_frame(trailer: &[u8]) -> Vec<u8> {
    frame_with_flags(FLAG_END_STREAM, trailer)
}

/// Accumulates transport chunks and yields whole envelopes. Chunk boundaries
/// are meaningless on this wire: one HTTP chunk may hold several frames, and
/// one frame may span many chunks.
#[derive(Debug, Default)]
pub struct FrameReader {
    buffer: Vec<u8>,
}

impl FrameReader {
    pub fn new() -> Self {
        Self::default()
    }

    /// Every frame that is complete after adding `chunk`.
    pub fn push(&mut self, chunk: &[u8]) -> Result<Vec<Frame>, ChannelError> {
        self.buffer.extend_from_slice(chunk);
        let mut frames = Vec::new();
        let mut consumed = 0_usize;
        while self.buffer.len() - consumed >= 5 {
            let header = &self.buffer[consumed..consumed + 5];
            let flags = header[0];
            let length = u32::from_be_bytes([header[1], header[2], header[3], header[4]]) as usize;
            if length > MAX_FRAME_BYTES {
                return Err(invalid(format!(
                    "Connect frame of {length} bytes exceeds the {MAX_FRAME_BYTES} byte limit"
                )));
            }
            if self.buffer.len() - consumed - 5 < length {
                break;
            }
            let start = consumed + 5;
            let payload = &self.buffer[start..start + length];
            let payload = if flags & FLAG_COMPRESSED == 0 {
                payload.to_vec()
            } else {
                inflate_gzip(payload)?
            };
            frames.push(Frame {
                end_stream: flags & FLAG_END_STREAM != 0,
                payload,
            });
            consumed = start + length;
        }
        self.buffer.drain(..consumed);
        Ok(frames)
    }

    /// Bytes held back because their frame is incomplete. A stream that ends
    /// with a partial frame was truncated.
    pub fn pending(&self) -> usize {
        self.buffer.len()
    }
}

/// A gzip member: the 10 byte fixed header, optional extra/name/comment/CRC
/// sections, the deflate stream, then CRC32 and ISIZE (RFC 1952). `miniz_oxide`
/// implements deflate but not this framing, so the header walk is here; the
/// CRC is not checked (the length check below already catches a truncated
/// member, and a corrupt one fails to inflate).
fn inflate_gzip(bytes: &[u8]) -> Result<Vec<u8>, ChannelError> {
    if bytes.len() < 18 || bytes[0] != 0x1f || bytes[1] != 0x8b {
        return Err(invalid(
            "Connect frame claims gzip but carries no gzip member",
        ));
    }
    if bytes[2] != 0x08 {
        return Err(invalid("gzip member does not use deflate"));
    }
    let flags = bytes[3];
    let mut offset = 10_usize;
    if flags & 0x04 != 0 {
        let extra = bytes
            .get(offset..offset + 2)
            .ok_or_else(|| invalid("truncated gzip extra field"))?;
        offset += 2 + usize::from(u16::from_le_bytes([extra[0], extra[1]]));
    }
    for skip_string in [flags & 0x08 != 0, flags & 0x10 != 0] {
        if skip_string {
            let end = bytes
                .get(offset..)
                .and_then(|rest| rest.iter().position(|byte| *byte == 0))
                .ok_or_else(|| invalid("unterminated gzip header string"))?;
            offset += end + 1;
        }
    }
    if flags & 0x02 != 0 {
        offset += 2;
    }
    let deflate = bytes
        .get(offset..bytes.len() - 8)
        .ok_or_else(|| invalid("truncated gzip member"))?;
    let inflated = miniz_oxide::inflate::decompress_to_vec_with_limit(deflate, MAX_FRAME_BYTES)
        .map_err(|error| invalid(format!("Connect frame decompression failed: {error:?}")))?;
    let size = &bytes[bytes.len() - 4..];
    let expected = u32::from_le_bytes([size[0], size[1], size[2], size[3]]);
    if expected != (inflated.len() as u64 % (1 << 32)) as u32 {
        return Err(invalid("gzip member length does not match its trailer"));
    }
    Ok(inflated)
}

/// `Basic <token>-<token>`: the session token doubled and dash-joined. A
/// single token is rejected upstream with `permission_denied`
/// (`devin-connect.js` file header §1, and the `authHeader` line on the
/// streaming path). The copy inside `ClientMetadata` stays single.
pub fn authorization(token: &str) -> Result<HeaderValue, ChannelError> {
    let token = token.trim();
    if token.is_empty() {
        return Err(ChannelError::InvalidCredential);
    }
    HeaderValue::from_str(&format!("Basic {token}-{token}"))
        .map_err(|_| ChannelError::InvalidCredential)
}

/// Headers for a Connect call carrying protobuf.
pub fn proto_headers(token: &str) -> Result<HeaderMap, ChannelError> {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static(PROTO_CONTENT_TYPE),
    );
    headers.insert(header::ACCEPT, HeaderValue::from_static("*/*"));
    common_headers(&mut headers);
    headers.insert(header::AUTHORIZATION, authorization(token)?);
    Ok(headers)
}

/// Headers for a Connect call carrying JSON. Connect's JSON codec is what the
/// account endpoints use; the session token rides in the body there, not in a
/// header (`windsurf-api.js::postJson` sends no authorization).
pub fn json_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    headers.insert(header::ACCEPT, HeaderValue::from_static("application/json"));
    common_headers(&mut headers);
    headers
}

fn common_headers(headers: &mut HeaderMap) {
    headers.insert(
        HeaderName::from_static("connect-protocol-version"),
        HeaderValue::from_static(PROTOCOL_VERSION),
    );
    headers.insert(
        HeaderName::from_static("connect-accept-encoding"),
        HeaderValue::from_static(ACCEPT_ENCODING),
    );
    headers.insert(header::USER_AGENT, HeaderValue::from_static(USER_AGENT));
}

/// The Connect code and message a failed trailer carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrailerError {
    pub code: Option<String>,
    pub message: String,
}

/// The trailer of a Connect stream, read for an error. `{}` and any
/// unparseable body are success; `{"error":{"code":…,"message":…}}` is not.
/// The code and the message are returned apart because both are inputs to the
/// taxonomy in `error.rs`, which needs them unflattened.
pub fn trailer_error(payload: &[u8]) -> Option<TrailerError> {
    let value: serde_json::Value = serde_json::from_slice(payload).ok()?;
    let error = value.get("error")?;
    let text = |key: &str| {
        error
            .get(key)
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    };
    Some(TrailerError {
        code: text("code"),
        message: text("message").unwrap_or_else(|| "upstream error".to_owned()),
    })
}

fn invalid(detail: impl Into<String>) -> ChannelError {
    ChannelError::InvalidResponse(detail.into())
}
