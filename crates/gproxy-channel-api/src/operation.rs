//! Transform-after operation orchestration without exposing transport.

use bytes::Bytes;
use serde_json::Value;

use crate::{ChannelError, Frame, PreparedRequest, StreamEnd};

pub struct StepResponse {
    pub status: http::StatusCode,
    pub headers: http::HeaderMap,
    pub body: Bytes,
}

pub enum DriverInput {
    Response(StepResponse),
    Continuation(Value),
}

pub enum OperationStep {
    Call {
        label: &'static str,
        request: Box<PreparedRequest>,
    },
    Claim {
        id: String,
    },
    Final {
        label: &'static str,
        request: Box<PreparedRequest>,
        stream: Box<dyn OperationStream>,
        cleanup: Box<PreparedRequest>,
        ttl_secs: u64,
    },
    Resume {
        stream: Box<dyn OperationStream>,
        cleanup: Box<PreparedRequest>,
        ttl_secs: u64,
    },
}

pub trait OperationDriver: Send {
    fn claim_id(&self) -> Option<&str> {
        None
    }

    fn next(&mut self, input: Option<DriverInput>) -> Result<OperationStep, ChannelError>;

    fn abort(&mut self) -> Option<PreparedRequest> {
        None
    }
}

pub struct StreamOutput {
    pub frames: Vec<Frame>,
    pub pause: Option<Pause>,
}

impl StreamOutput {
    pub fn frames(frames: Vec<Frame>) -> Self {
        Self {
            frames,
            pause: None,
        }
    }
}

pub struct Pause {
    pub id: String,
    pub state: Value,
    pub pending: Vec<Bytes>,
}

pub trait OperationStream: Send {
    fn push(&mut self, chunk: Bytes) -> Result<StreamOutput, ChannelError>;
    fn finish(&mut self, end: StreamEnd) -> Result<Vec<Frame>, ChannelError>;
}
