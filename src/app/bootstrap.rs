//! First-boot admin bootstrap (§14.2): a self-serviceable empty store.
//!
//! After persistence health + first-boot import, this either creates a random
//! `admin` under org `default` (printed once) on a fresh store, or — when a
//! `--admin-password`/`GPROXY_ADMIN_PASSWORD` override is set — force-upserts
//! that user every startup (host-level password recovery). Never seeds an
//! already-populated store unless the override is active. native main-path.

use crate::crypto::password;
use crate::store::persistence::PersistenceBackend;
use crate::store::persistence::records::{OrgInput, RoutePermissionInput, Scope, UserInput};

/// Ensure an admin exists. With `admin_password` set, force-upsert the named
/// user every call (recovery); otherwise seed a random admin only when the
/// `users` table is empty. The plaintext password is never persisted, cached,
/// or logged.
pub async fn ensure_admin(
    db: &dyn PersistenceBackend,
    admin_user: &str,
    admin_password: Option<&str>,
) -> anyhow::Result<()> {
    let org_id = ensure_default_org(db).await?;
    let users = db.list_users().await?;
    let existing = users.iter().find(|u| u.name == admin_user);

    if let Some(pw) = admin_password {
        // Host-level credential override: reclaim/create the admin every boot.
        // Same policy gate as the admin API — blank recovery passwords fail the
        // boot loudly instead of disabling password login by accident.
        password::validate_new(pw)
            .map_err(|e| anyhow::anyhow!("GPROXY_ADMIN_PASSWORD rejected: {e}"))?;
        let input = match existing {
            Some(u) => UserInput {
                id: Some(u.id),
                name: admin_user.to_string(),
                org_id: u.org_id,
                team_id: u.team_id,
                password: Some(password::hash(pw)?),
                enabled: true,
                is_admin: true,
            },
            None => UserInput {
                id: None,
                name: admin_user.to_string(),
                org_id,
                team_id: None,
                password: Some(password::hash(pw)?),
                enabled: true,
                is_admin: true,
            },
        };
        let user = db.upsert_user(input).await?;
        ensure_admin_route_permission(db, user.id).await?;
        tracing::warn!(
            admin_user,
            "admin credential override active (from env/CLI); REMOVE the env/flag after recovery"
        );
        return Ok(());
    }

    // No override: only bootstrap on a genuinely empty users table.
    if !users.is_empty() {
        return Ok(());
    }

    let pw = crate::util::rand::password();
    let user = db
        .upsert_user(UserInput {
            id: None,
            name: admin_user.to_string(),
            org_id,
            team_id: None,
            password: Some(password::hash(&pw)?),
            enabled: true,
            is_admin: true,
        })
        .await?;
    ensure_admin_route_permission(db, user.id).await?;
    print_admin_banner(admin_user, &pw);
    Ok(())
}

async fn ensure_admin_route_permission(
    db: &dyn PersistenceBackend,
    user_id: i64,
) -> anyhow::Result<()> {
    let permissions = db.list_route_permissions(Scope::User, user_id).await?;
    if permissions.iter().any(|p| p.route_pattern == "*") {
        return Ok(());
    }

    db.upsert_route_permission(RoutePermissionInput {
        id: None,
        scope: Scope::User,
        scope_id: user_id,
        route_pattern: "*".to_string(),
    })
    .await?;
    Ok(())
}

/// Resolve the `default` org, creating it if absent. Returns its id.
async fn ensure_default_org(db: &dyn PersistenceBackend) -> anyhow::Result<i64> {
    if let Some(org) = db.get_org_by_name("default").await? {
        return Ok(org.id);
    }
    let org = db
        .upsert_org(OrgInput {
            id: None,
            name: "default".to_string(),
            enabled: true,
            description: None,
        })
        .await?;
    Ok(org.id)
}

/// Print the one-time admin credentials to stdout in an unmissable box.
fn print_admin_banner(admin_user: &str, password: &str) {
    let line = "=".repeat(60);
    println!(
        "{line}\n GPROXY first-boot admin created\n   user:     {admin_user}\n   password: {password}\n Change it after first login. Shown ONCE — not stored in plaintext.\n{line}"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use argon2::password_hash::PasswordHash;

    async fn store() -> (tempfile::TempDir, crate::store::persistence::DbPersistence) {
        let dir = tempfile::tempdir().expect("tempdir");
        let fp = crate::store::persistence::DbPersistence::connect("sqlite::memory:")
            .await
            .expect("connect");
        (dir, fp)
    }

    #[tokio::test]
    async fn empty_store_creates_admin() {
        let (_dir, db) = store().await;
        ensure_admin(&db, "admin", None).await.unwrap();

        let users = db.list_users().await.unwrap();
        assert_eq!(users.len(), 1);
        let u = &users[0];
        assert_eq!(u.name, "admin");
        assert!(u.is_admin);
        assert!(u.enabled);
        let phc = u.password.as_ref().expect("password set");
        assert!(PasswordHash::new(phc).is_ok(), "stored a valid PHC");
        assert!(db.get_org_by_name("default").await.unwrap().is_some());

        // Re-running on the now-populated store must not duplicate the admin.
        ensure_admin(&db, "admin", None).await.unwrap();
        assert_eq!(db.list_users().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn override_resets_user() {
        let (_dir, db) = store().await;
        let org_id = ensure_default_org(&db).await.unwrap();
        // Seed a disabled, non-admin user named "admin".
        db.upsert_user(UserInput {
            id: None,
            name: "admin".to_string(),
            org_id,
            team_id: None,
            password: Some(password::hash("old").unwrap()),
            enabled: false,
            is_admin: false,
        })
        .await
        .unwrap();

        ensure_admin(&db, "admin", Some("recover-123456"))
            .await
            .unwrap();

        let users = db.list_users().await.unwrap();
        assert_eq!(users.len(), 1);
        let u = &users[0];
        assert!(u.is_admin);
        assert!(u.enabled);
        assert!(password::verify(
            "recover-123456",
            u.password.as_ref().unwrap()
        ));

        // Short recovery passwords are allowed; only blank/whitespace is
        // rejected so an env typo cannot silently disable password login.
        ensure_admin(&db, "admin", Some("x")).await.unwrap();
        let users = db.list_users().await.unwrap();
        assert!(password::verify("x", users[0].password.as_ref().unwrap()));

        let err = ensure_admin(&db, "admin", Some("   ")).await.unwrap_err();
        assert!(err.to_string().contains("must not be blank"), "got {err}");
    }
}
