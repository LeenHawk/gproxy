//! The IPC surface: one command per operation, and nothing else.
//!
//! # What an IPC command is allowed to do
//!
//! Exactly three things: unpack its arguments, call **one** method on
//! [`Operations`](gproxy_app::Operations), [`Portal`](gproxy_app::Portal),
//! `gproxy.manage()`, `gproxy.query()` or `gproxy.login()`, and render the
//! answer. No validation, no authorization, no branching on what came back.
//! Every rule already has an owner below this line, and a second opinion here
//! would be a rule that applies over IPC and not over HTTP — which is how a
//! desktop build and a server build start behaving differently.
//!
//! That is why [`table`] is generated from a declaration rather than written
//! by hand. A macro cannot smuggle a policy decision into a handler, and the
//! declaration is short enough to read end to end, which is the thing a router
//! of two hundred hand-written functions is not.
//!
//! # The table mirrors the operations, not the routes
//!
//! `gproxy-host-axum` maps HTTP paths to the same operations. The two surfaces
//! are siblings over one product layer, not one wrapping the other, so this
//! table is keyed on **families and methods** — `admin_users_list` is
//! `Operations::users().list(..)` — and never on a URL. `tests/table.rs`
//! checks the coverage against the families `Operations`, `Portal`, `Manage`
//! and `Query` expose, for the same reason: a route list is one host's
//! decision, and keying on it would make the desktop shell break whenever the
//! server's paths moved.
//!
//! # Authentication
//!
//! There is none, and that is the design. A message arrives here only because
//! this process's own webview sent it, so the channel is the proof; adding a
//! key check would be asking the application to authenticate itself to itself.
//! [`Desktop::caller`](crate::Desktop::caller) is the local administrator, and
//! the portal commands are scoped to them by construction exactly as they are
//! for a signed-in browser.
//!
//! The socket is a different matter. See [`crate::dataplane`].
//!
//! # The console does not call this table
//!
//! The console in the window sends its HTTP requests through one command,
//! `desktop_console_request`, which runs them through the server's router in
//! process; see [`crate::console`]. This table is for callers that want an
//! operation by name instead of a path.

pub mod table;

pub use table::{OPERATIONS, invoke_handler};

use crate::{Desktop, IpcError, IpcResult};

/// One entry in the command table: what it is called over IPC, and which
/// operation it calls.
///
/// The three descriptive fields are what makes coverage checkable. A test that
/// only knew command names could tell you that `admin_users_list` exists; one
/// that knows it is `Operations::users().list(..)` can tell you that a family
/// `gproxy-app` grew last week has no command at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Operation {
    /// The name the webview invokes.
    pub command: &'static str,
    /// `admin`, `portal`, `manage`, `query`, `upstream` or `desktop`.
    pub surface: &'static str,
    /// The receiver family.
    pub group: &'static str,
    /// The method called on that family.
    pub method: &'static str,
}

impl Operation {
    /// The family accessor this command reaches: `users` for
    /// `Operations::users()`, `login` for `Gproxy::login()`.
    pub const fn family(&self) -> &'static str {
        self.group
    }
}

/// One operation's answer, as JSON.
///
/// Every command returns `serde_json::Value` rather than its DTO, for the
/// reason the HTTP host renders bytes: this is a serialization boundary, and
/// the type that crosses it is described by ts-rs on the DTO itself — the same
/// declaration the HTTP console reads. Naming a hundred and twenty distinct
/// return types in the table would buy nothing on the far side of the bridge,
/// where everything is JSON regardless, and would make the table unreadable.
pub fn json<T, E>(result: Result<T, E>) -> IpcResult<serde_json::Value>
where
    T: serde::Serialize,
    E: Into<IpcError>,
{
    let value = result.map_err(Into::into)?;
    serde_json::to_value(value)
        .map_err(|error| IpcError::internal(format!("serializing the answer: {error}")))
}

/// The receiver a command calls its one method on.
///
/// Five preambles, one per surface, and each is the whole difference between
/// the commands built over it. The snapshot in the two `Operations` arms is a
/// local so that it lives for the call and not a moment longer: that is this
/// crate's half of the one-load-per-request rule both HTTP hosts follow.
///
/// Four of the five end in [`settle`], which is this host's half of
/// `gproxy-app`'s write contract: an operation commits its rows and tells the
/// peers, and the host rebuilds the identity snapshot. Without it a key minted
/// by `admin_api_keys_create` cannot authenticate on the data plane this same
/// process is serving, and a permission the window just granted is still
/// refused — until the next poll, or a restart. `query` is the exception and
/// the reason is in the sdk: nothing in that family writes, so none of it can
/// move the revision.
macro_rules! ipc_call {
    (admin, $desktop:ident, [$($chain:tt)*], $method:ident, ($($argument:expr,)*)) => {{
        let answer = {
            let data = $desktop.app().data();
            let operations = $desktop.operations(&data);
            $crate::ipc::json(operations $($chain)* .$method($($argument),*).await)
        };
        $crate::ipc::settle($desktop).await;
        answer
    }};
    (portal, $desktop:ident, [$($chain:tt)*], $method:ident, ($($argument:expr,)*)) => {{
        let answer = {
            let data = $desktop.app().data();
            let operations = $desktop.operations(&data);
            $crate::ipc::json(
                operations.portal($desktop.caller()) $($chain)* .$method($($argument),*).await
            )
        };
        $crate::ipc::settle($desktop).await;
        answer
    }};
    (manage, $desktop:ident, [$($chain:tt)*], $method:ident, ($($argument:expr,)*)) => {{
        let answer = $crate::ipc::json(
            $desktop.app().gproxy().manage() $($chain)* .$method($($argument),*).await
        );
        $crate::ipc::settle($desktop).await;
        answer
    }};
    (query, $desktop:ident, [$($chain:tt)*], $method:ident, ($($argument:expr,)*)) => {{
        $crate::ipc::json(
            $desktop.app().gproxy().query() $($chain)* .$method($($argument),*).await
        )
    }};
    (upstream, $desktop:ident, [$($chain:tt)*], $method:ident, ($($argument:expr,)*)) => {{
        let answer = $crate::ipc::json(
            $desktop.app().gproxy() $($chain)* .$method($($argument),*).await
        );
        $crate::ipc::settle($desktop).await;
        answer
    }};
}

/// IPC management/query operations follow the HTTP audit policy. Do not retain
/// command arguments or return values: either may contain credentials.
pub async fn audit(desktop: &Desktop, action: &str, result: &IpcResult<serde_json::Value>) {
    let mut entry = gproxy_app::AuditEntry::new(action).by(desktop.caller());
    entry.detail = match result {
        Ok(_) => serde_json::json!({ "transport": "ipc", "status": 200 }),
        Err(error) => {
            entry.outcome = "error".into();
            serde_json::json!({ "transport": "ipc", "status": error.status, "code": error.code })
        }
    };
    gproxy_app::Audit::new(
        desktop.app().gproxy().store(),
        desktop.app().config().audit_enabled,
    )
    .try_record(entry)
    .await;
}

/// Rebuild the identity snapshot if the command that just ran moved the
/// revision.
///
/// One read of `settings.config_revision`, and a rebuild only when that number
/// is ahead of the one being served — so a `list` costs a single indexed row
/// on a local SQLite file and nothing else. It is not classified by method
/// name on purpose: "which of these hundred and twenty commands writes" would
/// be a second opinion about what a write is, maintained here, and a new
/// family that got it wrong would be invisible until somebody noticed their
/// key did not work.
///
/// A failure is a warning: the write landed, and the poll or a peer's
/// notification brings this instance to it.
pub async fn settle(desktop: &Desktop) {
    if let Err(error) = desktop.app().sync_now().await {
        tracing::warn!(%error, "the identity snapshot was not refreshed after an operation");
    }
}

/// The command table.
///
/// One group per family:
///
/// ```text
/// <surface> <family> [<receiver chain>] { <method>…; }
/// ```
///
/// and one line per method:
///
/// ```text
/// list(query: ListQuery);              // one owned argument
/// get[id];                             // one borrowed string
/// update[id](patch: UserPatch);        // both
/// set_role[organization_id, user_id](patch: MemberPatch);
/// context;                             // neither
/// ```
///
/// The bracketed names become `String` parameters passed as `&str`, the
/// parenthesised ones are deserialized and passed by value, and the command's
/// IPC name is `<surface>_<family>_<method>`.
///
/// An `@manual { … }` block at the end names the handful of methods whose shape
/// the grammar above does not cover: a slice argument, a clock the webview
/// must not supply, a synchronous accessor that returns no `Result`. They are
/// ordinary `#[tauri::command]` functions, and listing them here is what keeps
/// them in the inventory and in the handler.
macro_rules! ipc_table {
    (
        $(
            $surface:ident $family:ident $chain:tt {
                $(
                    $method:ident
                    $([ $($borrowed:ident),+ ])?
                    $(( $($argument:ident : $type:ty),+ ))?
                ;)+
            }
        )+
        @manual {
            $( $manual_surface:ident $manual_family:ident $manual_method:ident
               => $manual_fn:ident ; )*
        }
    ) => {
        $( $(
            ::paste::paste! {
                #[doc = concat!(
                    "`", stringify!($surface), "` → `",
                    stringify!($family), ".", stringify!($method), "`."
                )]
                #[tauri::command]
                pub async fn [< $surface _ $family _ $method >](
                    desktop: ::tauri::State<'_, $crate::Desktop>,
                    $( $($borrowed: String,)+ )?
                    $( $($argument: $type,)+ )?
                ) -> $crate::IpcResult<::serde_json::Value> {
                    let desktop = &*desktop;
                    let answer = ipc_call!(
                        $surface, desktop, $chain, $method,
                        ( $( $(&$borrowed,)+ )? $( $($argument,)+ )? )
                    );
                    $crate::ipc::audit(desktop,
                        concat!(stringify!($surface), ".", stringify!($family), ".", stringify!($method)),
                        &answer).await;
                    answer
                }
            }
        )+ )+

        /// Every operation this host binds, in table order.
        ///
        /// The inventory is generated from the same declaration as the
        /// commands, so it cannot describe a command that does not exist or
        /// miss one that does. `tests/table.rs` reads it.
        pub const OPERATIONS: &[Operation] = &[
            $( $( ::paste::paste! {
                Operation {
                    command: stringify!([< $surface _ $family _ $method >]),
                    surface: stringify!($surface),
                    group: stringify!($family),
                    method: stringify!($method),
                }
            }, )+ )+
            $(
                Operation {
                    command: stringify!($manual_fn),
                    surface: stringify!($manual_surface),
                    group: stringify!($manual_family),
                    method: stringify!($manual_method),
                },
            )*
        ];

        /// The handler Tauri dispatches on, built from the same list.
        ///
        /// Generic over the runtime rather than fixed to `Wry`, so that
        /// `tests/ipc.rs` can build the very same table over Tauri's
        /// `MockRuntime` and invoke commands on a machine with no display
        /// server. A test that had to construct its own handler would not be
        /// testing this one.
        pub fn invoke_handler<R: ::tauri::Runtime>()
        -> impl Fn(::tauri::ipc::Invoke<R>) -> bool + Send + Sync + 'static {
            ::paste::paste! {
                ::tauri::generate_handler![
                    $( $( [< $surface _ $family _ $method >], )+ )+
                    $( $manual_fn, )*
                ]
            }
        }
    };
}

pub(crate) use {ipc_call, ipc_table};
