use super::stream::make_stream;
use super::*;
use futures_util::{SinkExt, lock::Mutex};
use protocol::{
    capability::UpstreamConnection,
    connection::{WsFrame, WsSender},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

pub struct RealtimeSession {
    pub sender: RealtimeSender,
    pub events: RealtimeStream,
}
impl RealtimeSession {
    pub fn split(self) -> (RealtimeSender, RealtimeStream) {
        (self.sender, self.events)
    }
}
pub(crate) struct State {
    pub outgoing: WsSender,
    pub model: RealtimeModel,
    pub auto_response: bool,
    pub vendor: Vendor,
    pub ready: bool,
    pub closed: bool,
    pub response_id: Option<String>,
    pub responding: bool,
    pub audio_item: Option<RealtimeItem>,
    pub response_audio_item: Option<RealtimeItem>,
    pub had_tool_calls: bool,
    pub interrupted: BTreeSet<RealtimeItem>,
    pub pending_history: Vec<Value>,
    pub output_format: AudioFormat,
    pub sequence: u64,
    pub input_active: bool,
    pub input_item: RealtimeItem,
    pub output_item: RealtimeItem,
    pub calls: BTreeMap<String, Option<String>>,
    pub input_text: String,
    pub input_done: bool,
    pub output_transcript: String,
    pub output_done: bool,
    pub output_text: String,
    pub turn_interrupted: bool,
    pub usage: RealtimeUsage,
    pub manual_input: bool,
    pub dialect: Driver,
}
#[derive(Clone, Copy)]
pub(crate) enum Driver {
    Openai,
    Gemini,
}
impl State {
    pub fn next_id(&mut self, prefix: &str) -> String {
        self.sequence += 1;
        format!("sdk-{prefix}-{}", self.sequence)
    }
    pub fn check_ready(&self) -> Result<()> {
        if self.closed {
            Err(Error::network("Realtime connection is closed"))
        } else if !self.ready {
            Err(Error::invalid(
                "poll the event stream until Ready before sending",
            ))
        } else {
            Ok(())
        }
    }
    pub async fn send(&mut self, value: impl Serialize) -> Result<()> {
        let text = serde_json::to_string(&value).map_err(Error::invalid)?;
        self.outgoing
            .send(WsFrame::Text(text))
            .await
            .map_err(|e| Error::network_cause(e.as_ref()))
    }
}
impl<B: Backend> Client<B> {
    pub async fn connect_realtime(&self, config: &RealtimeConfig) -> Result<RealtimeSession> {
        let vendor = config.model.dialect.vendor()?;
        let (
            first,
            mut warnings,
            history,
            operation,
            path,
            query,
            output_format,
            driver,
            manual_input,
        ) = match config.model.dialect {
            RealtimeDialect::Openai => {
                let mut rendered = openai::render(config)?;
                let history = openai::history(config, &mut rendered.warnings)?;
                let output_format =
                    openai_output_format(&rendered.request, &config.output_audio_format);
                (
                    json(rendered.request)?,
                    rendered.warnings,
                    history,
                    protocol::Dialect::OpenAi,
                    "/v1/realtime",
                    Some(format!(
                        "model={}",
                        crate::client::path_segment(&config.model.id)
                    )),
                    output_format,
                    Driver::Openai,
                    false,
                )
            }
            RealtimeDialect::Gemini => {
                let mut rendered = gemini::render(config)?;
                let history = gemini::history(config, &mut rendered.warnings)?
                    .into_iter()
                    .collect();
                let manual_input = rendered
                    .request
                    .setup
                    .as_ref()
                    .and_then(|s| s.realtime_input_config.as_ref())
                    .and_then(|r| r.automatic_activity_detection.as_ref())
                    .and_then(|d| d.disabled)
                    == Some(true);
                (
                    json(rendered.request)?,
                    rendered.warnings,
                    history,
                    protocol::Dialect::Gemini,
                    "/ws/google.ai.generativelanguage.v1beta.GenerativeService.BidiGenerateContent",
                    None,
                    config.output_audio_format.clone(),
                    Driver::Gemini,
                    manual_input,
                )
            }
            RealtimeDialect::Unknown(_) => {
                return Err(Error::unsupported("unknown Realtime dialect"));
            }
        };
        let request = WireRequest {
            method: http::Method::GET,
            path: path.into(),
            query,
            headers: http::HeaderMap::new(),
            body: (),
        };
        let connection = self
            .backend
            .connect(
                OperationKey {
                    dialect: operation,
                    operation: protocol::Operation::ConnectRealtime,
                },
                &config.model.id,
                request,
            )
            .await?;
        let socket = match connection {
            UpstreamConnection::Connected { socket, .. } => socket,
            UpstreamConnection::Rejected(response) => {
                let body = protocol::codec::read_http_body(response.body, DEFAULT_LIMITS).await?;
                return Err(Error::from_response(
                    response.status.as_u16(),
                    &response.headers,
                    String::from_utf8_lossy(&body).into_owned(),
                ));
            }
        };
        let mut state = State {
            outgoing: socket.outgoing,
            model: config.model.clone(),
            auto_response: config.auto_response,
            vendor: vendor.clone(),
            ready: false,
            closed: false,
            response_id: None,
            responding: false,
            audio_item: None,
            response_audio_item: None,
            had_tool_calls: false,
            interrupted: BTreeSet::new(),
            pending_history: history,
            output_format,
            sequence: 0,
            input_active: false,
            input_item: RealtimeItem::new("live-input-0", 0),
            output_item: RealtimeItem::new("live-output-0", 0),
            calls: BTreeMap::new(),
            input_text: String::new(),
            input_done: false,
            output_transcript: String::new(),
            output_done: false,
            output_text: String::new(),
            turn_interrupted: false,
            usage: RealtimeUsage::default(),
            dialect: driver,
            manual_input,
        };
        state.send(first).await?;
        let shared = Arc::new(Mutex::new(state));
        let queue = Arc::new(queue::Queue::default());
        for warning in warnings.drain(..) {
            queue.push(RealtimeEvent::local(
                vendor.clone(),
                RealtimeEventKind::Warning { warning },
            ));
        }
        let sender = RealtimeSender {
            state: shared.clone(),
            queue: queue.clone(),
        };
        Ok(RealtimeSession {
            sender,
            events: make_stream(socket.incoming, shared, queue),
        })
    }
}
pub(crate) fn openai_output_format(
    update: &protocol::openai::realtime::SessionUpdateEvent,
    fallback: &AudioFormat,
) -> AudioFormat {
    use protocol::openai::realtime::SessionUpdateEventSession as S;
    if let S::RealtimeSessionCreateRequest(session) = &update.session
        && let Some(format) = session
            .audio
            .as_ref()
            .and_then(|a| a.as_ref())
            .and_then(|a| a.output.as_ref())
            .and_then(|o| o.as_ref())
            .and_then(|o| o.format.as_ref())
            .and_then(|f| f.as_ref())
    {
        return openai::format_from_wire(format);
    }
    fallback.clone()
}
