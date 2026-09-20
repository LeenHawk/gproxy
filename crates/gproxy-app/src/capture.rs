//! Downstream request/response capture: the `capture_records` row for the
//! caller's own exchange, and the `CaptureLink` rows that tie it to the
//! upstream attempts core recorded, including the ones that were retried
//! elsewhere. Lands in P5.
