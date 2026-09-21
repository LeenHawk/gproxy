//! The IPC table covers the same operations the product layer exposes.
//!
//! # What this is keyed on, and what it is deliberately not keyed on
//!
//! The comparison is against the **families** `gproxy-app` and `gproxy-sdk`
//! declare — the `pub fn users(&self)` accessors on `Operations`, `Portal`,
//! `Manage` and `Query` — and never against `gproxy-host-axum`'s route table.
//!
//! That is not a convenience. The HTTP host's paths are its own decision: it
//! may nest a family under a prefix, split one across two routes or serve two
//! families from one. A test that asserted "one IPC command per HTTP route"
//! would fail every time somebody reorganised a router, and would pass while
//! a family that no host had bound yet went missing from both. The operations
//! are the contract both hosts implement, so the operations are what is
//! checked.
//!
//! # How it reads them
//!
//! From the source of the two `mod.rs` files that declare the accessors. A
//! Rust test cannot enumerate a type's methods — there is no reflection — and
//! the alternative is a hand-maintained list in this file, which is a copy
//! that goes stale exactly when it matters: the day a family is added.
//! Reading the declaration means a new `pub fn audit(&self)` in
//! `gproxy-app` fails this test the moment it lands.
//!
//! The paths are resolved from `CARGO_MANIFEST_DIR`, which cargo sets, so
//! this works from any working directory.

use gproxy_host_tauri::{OPERATIONS, Operation};
use std::collections::BTreeSet;

/// The four declaration files, and the type in each whose accessors are the
/// families a host binds.
const SURFACES: [(&str, &str, &str); 4] = [
    ("admin", "gproxy-app/src/operations/mod.rs", "Operations"),
    (
        "portal",
        "gproxy-app/src/operations/portal/mod.rs",
        "Portal",
    ),
    ("manage", "gproxy-sdk/src/manage/mod.rs", "Manage"),
    ("query", "gproxy-sdk/src/query/mod.rs", "Query"),
];

/// Accessors that are not families, with the reason each one is not.
///
/// Every entry here is a decision somebody made on purpose. Adding to this
/// list is how a family is excluded; forgetting to add to it is how this test
/// tells you that you have not decided.
const NOT_A_FAMILY: [(&str, &str); 13] = [
    // Constructors and borrowed context, not operations.
    ("admin", "new"),
    ("admin", "data"),
    ("admin", "config"),
    ("admin", "store"),
    ("portal", "new"),
    ("portal", "caller"),
    ("manage", "new"),
    ("query", "new"),
    // `Scope::name()`, the string a peer sees in an invalidation. It shares
    // a file with the families and is not one.
    ("manage", "name"),
    // `Operations::portal(&caller)` builds the end user's surface; every
    // `portal_*` command in the table goes through it, so it is the door to
    // a surface rather than a family behind one. Declared in the portal
    // module, which is why it is keyed there.
    ("portal", "portal"),
    // Sign-in and sign-out for a browser session. The desktop window has no
    // session to establish: the IPC channel is the trust boundary, and
    // `Desktop::caller` is the local administrator by construction.
    ("portal", "portal_login"),
    ("portal", "portal_logout"),
    // The OAuth authorization server this instance runs for *downstream*
    // clients. It is reached over HTTP at `/v1/oauth/*` by programs that
    // redirect a browser at it; invoking `token` over the IPC bridge of the
    // application that issues it has no meaning, and the embedded data plane
    // already serves the real endpoints.
    ("admin", "issuer"),
];

/// Groups whose "family" is the surface type itself rather than an accessor
/// on it.
///
/// `Portal` has methods of its own — `context`, `usage`, `quota` — that no
/// accessor leads to, and they are bound as `portal_me_*`. Naming the group
/// `me` is what makes an end user's own overview read like one.
const SURFACE_ITSELF: [(&str, &str); 1] = [("portal", "me")];

/// The `pub fn <name>(&self)` accessors declared in one file.
///
/// Deliberately crude — a line-oriented match on the accessor shape rather
/// than a parse. A family accessor is one line in this codebase, always
/// `pub fn name(&self) -> Type<'a, C>`, and a test that needed `syn` to find
/// it would be a test nobody trusts to have read the file correctly.
fn accessors(relative: &str) -> BTreeSet<String> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join(relative);
    let source = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("reading {}: {error}", path.display()));
    source
        .lines()
        .map(str::trim)
        .filter_map(|line| {
            let rest = line.strip_prefix("pub fn ")?;
            let (name, rest) = rest.split_once('(')?;
            // `(&self)` and `(&self, …)`: an accessor takes the receiver and
            // nothing that identifies a row. `Operations::portal(&caller)`
            // is on the exemption list rather than excluded here, because
            // being a constructor is a decision worth writing down.
            rest.starts_with("&self").then(|| name.to_owned())
        })
        .collect()
}

fn bound(surface: &str) -> BTreeSet<&'static str> {
    OPERATIONS
        .iter()
        .filter(|operation| operation.surface == surface)
        .map(Operation::family)
        .collect()
}

#[test]
fn every_family_the_product_layer_exposes_has_a_command() {
    let exempt: BTreeSet<(&str, &str)> = NOT_A_FAMILY.into_iter().collect();
    let mut missing = Vec::new();

    for (surface, file, type_name) in SURFACES {
        let bound = bound(surface);
        for accessor in accessors(file) {
            if exempt.contains(&(surface, accessor.as_str())) {
                continue;
            }
            // A family may be reached by several groups — `manage rewrite`,
            // `manage rewrite_sets`, `manage rewrite_rules` are all
            // `Manage::rewrite()` — so a prefix counts.
            let covered = bound
                .iter()
                .any(|family| *family == accessor || family.starts_with(&format!("{accessor}_")));
            if !covered {
                missing.push(format!(
                    "{type_name}::{accessor}() ({file}) has no `{surface}_{accessor}_*` command"
                ));
            }
        }
    }

    assert!(
        missing.is_empty(),
        "the IPC table has fallen behind the operations it mirrors:\n  {}\n\nAdd a group to \
         `ipc::table`, or add the accessor to NOT_A_FAMILY in this file with the reason it is \
         not an operation.",
        missing.join("\n  ")
    );
}

#[test]
fn no_command_names_a_family_that_does_not_exist() {
    // The other direction: a group in the table whose accessor was renamed or
    // removed would otherwise keep a dead command alive.
    let itself: BTreeSet<(&str, &str)> = SURFACE_ITSELF.into_iter().collect();
    let mut orphans = Vec::new();
    for (surface, file, _) in SURFACES {
        let accessors = accessors(file);
        for family in bound(surface) {
            if itself.contains(&(surface, family)) {
                continue;
            }
            let known = accessors
                .iter()
                .any(|accessor| family == accessor || family.starts_with(&format!("{accessor}_")));
            if !known {
                orphans.push(format!("{surface}_{family}_*"));
            }
        }
    }
    assert!(
        orphans.is_empty(),
        "these command groups name a family that no longer exists: {orphans:?}"
    );
}

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
    assert_eq!(seen.len(), OPERATIONS.len());
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

#[test]
fn the_table_is_the_whole_management_surface_not_a_sample() {
    // A floor rather than an exact count, so that adding an operation does
    // not fail a test about coverage. The number is here to catch the
    // opposite: a refactor that drops half the table and still passes every
    // structural check above.
    assert!(
        OPERATIONS.len() > 150,
        "only {} commands; the management surface is larger than that",
        OPERATIONS.len()
    );
}

/// The table, printed. Not an assertion — a way to read what this host binds
/// without expanding a macro:
///
/// ```sh
/// cargo test -p gproxy-host-tauri --test table -- --ignored --nocapture
/// ```
#[test]
#[ignore = "prints the table; it asserts nothing"]
fn print_the_table() {
    let mut surfaces: std::collections::BTreeMap<&str, usize> = Default::default();
    for operation in OPERATIONS {
        println!(
            "{:<10} {:<28} {:<24} {}",
            operation.surface, operation.group, operation.method, operation.command
        );
        *surfaces.entry(operation.surface).or_default() += 1;
    }
    println!("\n{} commands", OPERATIONS.len());
    for (surface, count) in surfaces {
        println!("  {surface:<10} {count}");
    }
}
