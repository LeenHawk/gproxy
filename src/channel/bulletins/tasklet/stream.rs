use std::collections::VecDeque;
use std::time::Duration;

use bytes::Bytes;
use futures_util::StreamExt;

use super::{bridge, response::Synth};
use crate::http::client::{ClientError, ConduitSocket, RespStream};

const IDLE_TIMEOUT: Duration = Duration::from_secs(180);
const TOTAL_DEADLINE_MS: u64 = 1_800_000;

struct State {
    socket: Box<dyn ConduitSocket>,
    synth: Synth,
    queued: VecDeque<Bytes>,
    deadline_ms: u64,
    finished: bool,
    turn: Option<bridge::Turn>,
}

pub fn create(
    socket: Box<dyn ConduitSocket>,
    synth: Synth,
    turn: Option<bridge::Turn>,
) -> Result<RespStream, ClientError> {
    let initial = synth
        .initial()
        .map_err(|error| ClientError::Transport(error.to_string()))?;
    let state = State {
        socket,
        synth,
        queued: VecDeque::from([initial]),
        deadline_ms: crate::util::time::unix_now_ms() + TOTAL_DEADLINE_MS,
        finished: false,
        turn,
    };
    Ok(
        futures_util::stream::unfold(Some(state), |state| async move {
            let mut state = state?;
            if let Some(chunk) = state.queued.pop_front() {
                let next = (!state.finished || !state.queued.is_empty()).then_some(state);
                return Some((Ok(chunk), next));
            }
            loop {
                if crate::util::time::unix_now_ms() >= state.deadline_ms {
                    return Some((
                        Err(ClientError::Transport(
                            "tasklet turn deadline exceeded".into(),
                        )),
                        None,
                    ));
                }
                enum Event {
                    Socket(Option<Result<String, ClientError>>),
                    Tool(Option<bridge::ToolInvocation>),
                }
                let event = tokio::time::timeout(IDLE_TIMEOUT, async {
                    if let Some(turn) = state.turn.as_mut() {
                        tokio::select! {
                            frame = state.socket.recv_text() => Event::Socket(frame),
                            invocation = turn.recv() => Event::Tool(invocation),
                        }
                    } else {
                        Event::Socket(state.socket.recv_text().await)
                    }
                })
                .await;
                let text = match event {
                    Ok(Event::Socket(Some(Ok(text)))) => text,
                    Ok(Event::Socket(Some(Err(error)))) => return Some((Err(error), None)),
                    Ok(Event::Socket(None)) => {
                        return Some((
                            Err(ClientError::Transport(
                                "tasklet websocket closed before completion".into(),
                            )),
                            None,
                        ));
                    }
                    Ok(Event::Tool(Some(invocation))) => {
                        let (name, payload, accepted) = bridge::into_parts(invocation);
                        match state.synth.tool_call(name, payload) {
                            Ok(absorbed) => {
                                let _ = accepted.send(());
                                state.finished = true;
                                state.queued.extend(absorbed.chunks);
                                let chunk = state.queued.pop_front().expect("tool call chunks");
                                return Some((Ok(chunk), Some(state)));
                            }
                            Err(error) => {
                                return Some((Err(ClientError::Transport(error)), None));
                            }
                        }
                    }
                    Ok(Event::Tool(None)) => {
                        state.turn = None;
                        continue;
                    }
                    Err(_) => {
                        return Some((
                            Err(ClientError::Transport(
                                "tasklet websocket idle timeout".into(),
                            )),
                            None,
                        ));
                    }
                };
                match state.synth.absorb(&text) {
                    Ok(absorbed) => {
                        state.finished = absorbed.finished;
                        state.queued.extend(absorbed.chunks);
                        if let Some(chunk) = state.queued.pop_front() {
                            let next =
                                (!state.finished || !state.queued.is_empty()).then_some(state);
                            return Some((Ok(chunk), next));
                        }
                    }
                    Err(error) => {
                        return Some((Err(ClientError::Transport(error)), None));
                    }
                }
            }
        })
        .boxed(),
    )
}
