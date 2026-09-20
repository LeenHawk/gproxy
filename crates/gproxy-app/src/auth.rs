//! Turning a request into a `Caller`. Lands in P2.
//!
//! Four ladders, all producing the same `Caller { user_id, user_role,
//! api_key_id, organization_id, team_id, subscription_id, grant, kind }`:
//!
//! - a gateway API key, hashed and looked up in [`crate::snapshot::ApiKeyIndex`]
//!   under both the raw text and the text with an `sk-` prefix removed, with a
//!   `kind = OAuth` key refused as a bearer key;
//! - an OAuth access token, resolved through the store's `resolve_access_many`
//!   so the grant's revocation is checked in the same statement;
//! - a password, verified with argon2;
//! - a console or portal session cookie backed by `user_sessions`, with
//!   same-origin CSRF checking on every non-GET.
