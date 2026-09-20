//! The typed management surface: `AdminOp`, `PortalOp` and `IssuerOp`, each
//! family written as inputs / validate / write / map, dispatched by one table
//! both hosts share. Writes go through `Store::commit_revision` so a change
//! and its revision bump are one transaction, and publish
//! `ConfigurationChanged { scopes }`; the scopes decide whether only `AppData`
//! is rebuilt or the engine reloads with it. Lands in P6 (identity), P7 (the
//! OAuth issuer) and P8 (portal).
//!
//! One rule that belongs to this layer and to no other: **deleting an
//! organization or a team must unbind or delete the API keys bound to it.**
//! The cascade declared on `api_keys.organization_id` / `team_id` only exists
//! in databases created from the current schema; the incremental SQLite
//! upgrade adds those columns with `ALTER TABLE ADD COLUMN`, which cannot
//! carry a foreign key. An upgraded instance would otherwise keep serving
//! requests on a key whose budget chain and visibility boundary no longer
//! exist.
