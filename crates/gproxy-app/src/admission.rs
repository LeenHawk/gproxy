//! Turning a `Caller` into an `Admitted`. Lands in P3.
//!
//! Permissions produce the allowed provider set, credential ownership the
//! allowed credential set, the key binding the budget owner chain
//! `[api_key?, user, subscription?, team?, org?]` and the scope
//! (`user:{id}` / `grant:{id}`), the rate limits a cache permit (a cache
//! failure refuses the request rather than passing it), and the session
//! ladder a `SessionIdentity`. Nothing downstream re-derives any of it.
