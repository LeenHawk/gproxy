//! Storage codecs. Callers supply already-redacted bytes; none of this runs in
//! the forwarding stream. Identity remains readable for existing databases.
use crate::entity::usage::capture_event::{CaptureDirection, CaptureEventKind};
use crate::{Result, error::invalid};
use std::time::Duration;
use web_time::Instant;

pub const SEGMENT_BYTES: usize = 64 * 1024;
pub const SEGMENT_IDLE: Duration = Duration::from_secs(1);

pub fn compress(bytes: &[u8]) -> Result<(String, Vec<u8>)> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let compressed = zstd::bulk::compress(bytes, 3).map_err(|e| invalid(e.to_string()))?;
        if compressed.len() < bytes.len() {
            return Ok(("zstd".into(), compressed));
        }
    }
    // Edge builds have no native zstd encoder. They can still read native
    // captures and write the same schema using identity encoding.
    Ok(("identity".into(), bytes.to_vec()))
}

pub fn decompress(encoding: &str, bytes: &[u8]) -> Result<Vec<u8>> {
    match encoding {
        "identity" => Ok(bytes.to_vec()),
        "zstd" => {
            #[cfg(not(target_arch = "wasm32"))]
            {
                zstd::stream::decode_all(bytes).map_err(|e| invalid(e.to_string()))
            }
            #[cfg(target_arch = "wasm32")]
            {
                use std::io::Read;
                let mut decoder = ruzstd::decoding::StreamingDecoder::new(bytes)
                    .map_err(|e| invalid(e.to_string()))?;
                let mut out = Vec::new();
                decoder
                    .read_to_end(&mut out)
                    .map_err(|e| invalid(e.to_string()))?;
                Ok(out)
            }
        }
        _ => Err(invalid(format!("unknown capture encoding {encoding}"))),
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Chunk {
    pub sequence: i64,
    pub turn_id: Option<String>,
    pub direction: CaptureDirection,
    pub kind: CaptureEventKind,
    pub payload: Vec<u8>,
    pub observed_at_ms: i64,
}

/// Consecutive chunks of one direction, kind and turn, concatenated. The
/// offset list is a pair of unsigned varints per chunk: end offset into the
/// payload and sequence delta from the head. Per-chunk arrival times are not
/// kept; every chunk of a segment reports the head's `observed_at_ms`, which
/// is within [`SEGMENT_IDLE`] of the truth by construction.
#[derive(Debug)]
pub struct Segment {
    pub head: Chunk,
    pub chunk_offsets: Vec<u8>,
    last_sequence: i64,
    touched: Instant,
}
impl Segment {
    fn new(chunk: Chunk) -> Self {
        let mut offsets = Vec::new();
        varint(chunk.payload.len() as u64, &mut offsets);
        varint(0, &mut offsets);
        Self {
            last_sequence: chunk.sequence,
            head: chunk,
            chunk_offsets: offsets,
            touched: Instant::now(),
        }
    }
    fn compatible(&self, chunk: &Chunk) -> bool {
        self.head.kind == chunk.kind
            && self.head.turn_id == chunk.turn_id
            && chunk.sequence > self.last_sequence
    }
    fn push(&mut self, chunk: Chunk) {
        self.head.payload.extend_from_slice(&chunk.payload);
        varint(self.head.payload.len() as u64, &mut self.chunk_offsets);
        varint(
            (chunk.sequence - self.head.sequence) as u64,
            &mut self.chunk_offsets,
        );
        self.last_sequence = chunk.sequence;
        self.touched = Instant::now();
    }
}

/// One pending segment per direction. A kind or turn transition closes the
/// segment so WS control/message/turn identity is never conflated. Sequence
/// deltas retain ordering even when opposite directions interleave.
#[derive(Default)]
pub struct Segments {
    pending: [Option<Segment>; 2],
}
impl Segments {
    pub fn push(&mut self, chunk: Chunk) -> Vec<Segment> {
        let index = usize::from(chunk.direction == CaptureDirection::Response);
        let slot = &mut self.pending[index];
        let mut ready = Vec::new();
        if slot.as_ref().is_some_and(|s| !s.compatible(&chunk)) {
            ready.push(slot.take().unwrap());
        }
        if let Some(segment) = slot {
            segment.push(chunk);
        } else {
            *slot = Some(Segment::new(chunk));
        }
        if slot
            .as_ref()
            .is_some_and(|s| s.head.payload.len() >= SEGMENT_BYTES)
        {
            ready.push(slot.take().unwrap());
        }
        ready
    }
    /// Time until the oldest pending segment goes idle; `None` when nothing is
    /// pending, so an idle writer can sleep until its next chunk arrives.
    pub fn deadline(&self) -> Option<Duration> {
        self.pending
            .iter()
            .flatten()
            .map(|s| SEGMENT_IDLE.saturating_sub(s.touched.elapsed()))
            .min()
    }
    pub fn drain(&mut self, all: bool) -> Vec<Segment> {
        self.pending
            .iter_mut()
            .filter_map(|slot| {
                if all
                    || slot
                        .as_ref()
                        .is_some_and(|s| s.touched.elapsed() >= SEGMENT_IDLE)
                {
                    slot.take()
                } else {
                    None
                }
            })
            .collect()
    }
}

fn varint(mut value: u64, out: &mut Vec<u8>) {
    while value >= 128 {
        out.push((value as u8 & 127) | 128);
        value >>= 7;
    }
    out.push(value as u8);
}
fn read_varint(bytes: &mut &[u8]) -> Result<u64> {
    let mut value = 0u64;
    for shift in (0..70).step_by(7) {
        let (&byte, rest) = bytes
            .split_first()
            .ok_or_else(|| invalid("truncated chunk offsets"))?;
        *bytes = rest;
        if shift == 63 && byte > 1 {
            return Err(invalid("chunk offset overflow"));
        }
        value |= u64::from(byte & 127) << shift;
        if byte & 128 == 0 {
            return Ok(value);
        }
    }
    Err(invalid("chunk offset overflow"))
}

pub fn split(head: Chunk, offsets: Option<&[u8]>) -> Result<Vec<Chunk>> {
    let Some(mut offsets) = offsets else {
        return Ok(vec![head]);
    };
    let mut out = Vec::new();
    let mut start = 0;
    let mut last_sequence = None;
    while !offsets.is_empty() {
        let end = usize::try_from(read_varint(&mut offsets)?)
            .map_err(|_| invalid("chunk offset overflow"))?;
        let sequence = head
            .sequence
            .checked_add(
                i64::try_from(read_varint(&mut offsets)?)
                    .map_err(|_| invalid("chunk sequence overflow"))?,
            )
            .ok_or_else(|| invalid("chunk sequence overflow"))?;
        if end < start
            || end > head.payload.len()
            || last_sequence.is_some_and(|last| sequence <= last)
        {
            return Err(invalid("invalid capture chunk offsets"));
        }
        out.push(Chunk {
            sequence,
            turn_id: head.turn_id.clone(),
            direction: head.direction,
            kind: head.kind,
            payload: head.payload[start..end].to_vec(),
            observed_at_ms: head.observed_at_ms,
        });
        last_sequence = Some(sequence);
        start = end;
    }
    if start != head.payload.len() || out.is_empty() {
        return Err(invalid("incomplete capture chunk offsets"));
    }
    Ok(out)
}
