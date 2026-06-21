//! Portal audit middleware: records an append-only row for every mutating
//! (non-GET) portal request. Mirrors `admin/audit.rs` but reads `SessionUser`
//! instead of `AdminUser`. Runs INNER to [`super::super::middleware::require_session`]
//! so the [`SessionUser`] extension is already set.
//!
//! The persistence write is fire-and-forget (`tokio::spawn`) so it never delays
//! the response.

use axum::extract::{Request, State};
use axum::http::Method;
use axum::middleware::Next;
use axum::response::Response;

use crate::admin::session::SessionUser;
use crate::app::AppState;
use crate::store::persistence::records::AuditLogInput;

/// Audit a mutating portal request. GET requests are skipped; everything else
/// is recorded after the handler runs.
pub async fn audit(State(state): State<AppState>, req: Request, next: Next) -> Response {
    if req.method() == Method::GET {
        return next.run(req).await;
    }

    let action = req.method().as_str().to_owned();
    let target = req.uri().path().to_owned();
    let source_ip = crate::http::server::admin::client_ip(
        req.headers(),
        crate::http::server::admin::peer_ip(req.extensions()),
        &state.config.trusted_proxies,
    );
    let (actor_id, actor_name) = match req.extensions().get::<SessionUser>() {
        Some(u) => (Some(u.id), Some(u.name.clone())),
        None => (None, None),
    };

    let resp = next.run(req).await;
    let status = resp.status().as_u16() as i64;

    let persistence = state.persistence.clone();
    tokio::spawn(async move {
        let input = AuditLogInput {
            actor_id,
            actor_name,
            action,
            target,
            status,
            source_ip,
        };
        if let Err(e) = persistence.append_audit_log(input).await {
            tracing::warn!("portal audit log write failed: {e}");
        }
    });

    resp
}
