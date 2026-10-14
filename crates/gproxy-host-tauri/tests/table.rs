//! IPC command uniqueness, naming and required entry points.

use gproxy_host_tauri::OPERATIONS;
use std::collections::BTreeSet;

#[test]
fn the_upstream_login_surface_is_the_handles_own_family() {
    // `upstream` is not one of the four types above: it is `Gproxy::login()`,
    // a peer of `manage()` and `query()` on the handle. Checked by name so
    // that renaming the surface is a deliberate act.
    let upstream: Vec<_> = OPERATIONS
        .iter()
        .filter(|operation| operation.surface == "upstream")
        .collect();
    assert!(!upstream.is_empty());
    assert!(
        upstream
            .iter()
            .all(|operation| operation.family() == "login")
    );
    for method in [
        "authcode_start",
        "authcode_complete",
        "device_start",
        "device_poll",
        "cookie_exchange",
    ] {
        assert!(
            upstream.iter().any(|operation| operation.method == method),
            "the desktop shell exists to run logins, and `{method}` is not bound"
        );
    }
}

#[test]
fn every_command_name_is_unique() {
    // Two commands with one name is a table where the second silently wins,
    // and Tauri's own dispatch would pick whichever the generated `match`
    // reached first.
    let mut seen = BTreeSet::new();
    for operation in OPERATIONS {
        assert!(
            seen.insert(operation.command),
            "`{}` is in the table twice",
            operation.command
        );
    }
}

#[test]
fn a_command_is_named_after_the_operation_it_calls() {
    // The naming rule the console relies on to map one operation name onto
    // either transport.
    for operation in OPERATIONS {
        assert_eq!(
            operation.command,
            format!(
                "{}_{}_{}",
                operation.surface, operation.group, operation.method
            ),
            "{operation:?}"
        );
    }
}

#[test]
fn the_five_method_shape_survived_the_table() {
    // A spot check that the generated table really is the families and not a
    // subset somebody trimmed: the CRUD five, on one family per surface.
    for (surface, family) in [
        ("admin", "users"),
        ("manage", "providers"),
        ("manage", "pricing_rates"),
    ] {
        for method in ["list", "get", "create", "update", "delete"] {
            let command = format!("{surface}_{family}_{method}");
            assert!(
                OPERATIONS
                    .iter()
                    .any(|operation| operation.command == command),
                "`{command}` is missing"
            );
        }
    }
}
