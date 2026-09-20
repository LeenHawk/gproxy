//! Reading back what happened: usage records, quota windows, credential
//! cycles, limits and captured exchanges.
//!
//! These are Store reads with pagination and Rust-side aggregation; nothing
//! here writes or advances a revision. Filled in by the query phase.
