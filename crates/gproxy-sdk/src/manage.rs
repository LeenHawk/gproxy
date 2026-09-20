//! Configuration writes: the families a management UI drives.
//!
//! Every write is one `Store::commit_revision` batch, so the rows and the
//! `config_revision` bump land together, followed by a local reload and an
//! `Invalidation::ConfigurationChanged` on the shared cache for the peers.
//! Filled in by the management phase.
