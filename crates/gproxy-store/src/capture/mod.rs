//! Capture storage transforms and atomic write plans. Repositories expose the
//! physical schema; capture consumers use the hydration methods here.
mod codec;
mod retention;
mod storage;
pub use codec::{
    Chunk, SEGMENT_BYTES, SEGMENT_IDLE, Segment, Segments, compress, decompress, split,
};
pub use retention::PayloadRetention;
pub use storage::{canonical_headers, header_hash, tenant_scope};
