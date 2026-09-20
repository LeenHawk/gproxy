//! Sending a request through the resolved plan.
//!
//! The builder collects what core needs for a `RequestContext` — scope,
//! attribution, budgets, session, deadline — and walks the plan's targets,
//! moving to the next provider only for failures that another provider could
//! serve. Filled in by the call phase.
