use super::build::{create_index, create_table};
use super::{Dialect, SchemaVersion, tables};

pub(super) fn statements(dialect: Dialect) -> Vec<String> {
    let mut statements = vec![
        "INSERT INTO organizations (name, enabled) SELECT 'default', 1 WHERE EXISTS (SELECT 1 FROM admin_accounts) AND NOT EXISTS (SELECT 1 FROM organizations WHERE name = 'default')".into(),
        "UPDATE users SET password_hash = (SELECT password_hash FROM admin_accounts WHERE username = users.name), is_admin = 1, enabled = (SELECT enabled FROM admin_accounts WHERE username = users.name), organization_id = COALESCE(organization_id, (SELECT id FROM organizations WHERE name = 'default')) WHERE EXISTS (SELECT 1 FROM admin_accounts WHERE username = users.name)".into(),
        "INSERT INTO users (name, organization_id, team_id, password_hash, enabled, is_admin) SELECT admin_accounts.username, organizations.id, NULL, admin_accounts.password_hash, admin_accounts.enabled, 1 FROM admin_accounts CROSS JOIN organizations WHERE organizations.name = 'default' AND NOT EXISTS (SELECT 1 FROM users WHERE users.name = admin_accounts.username)".into(),
        "CREATE TABLE wave26_admin_user_map (admin_id BIGINT NOT NULL PRIMARY KEY, user_id BIGINT NOT NULL)".into(),
        "INSERT INTO wave26_admin_user_map (admin_id, user_id) SELECT admin_accounts.id, users.id FROM admin_accounts INNER JOIN users ON users.name = admin_accounts.username".into(),
        "INSERT INTO permissions (subject_kind, subject_id, provider_id, operation_group, allowed) SELECT 'user', wave26_admin_user_map.user_id, NULL, NULL, 1 FROM wave26_admin_user_map WHERE NOT EXISTS (SELECT 1 FROM permissions WHERE permissions.subject_kind = 'user' AND permissions.subject_id = wave26_admin_user_map.user_id AND permissions.provider_id IS NULL AND permissions.operation_group IS NULL AND permissions.allowed = 1)".into(),
        "INSERT INTO user_sessions (token_digest, user_id, created_at, expires_at) SELECT admin_sessions.token_digest, wave26_admin_user_map.user_id, admin_sessions.created_at, admin_sessions.expires_at FROM admin_sessions INNER JOIN wave26_admin_user_map ON wave26_admin_user_map.admin_id = admin_sessions.admin_id".into(),
        "INSERT INTO user_keys (user_id, digest, label, expires_at, enabled, digest_version) SELECT wave26_admin_user_map.user_id, admin_api_keys.digest, NULL, NULL, 1, 1 FROM admin_api_keys INNER JOIN wave26_admin_user_map ON wave26_admin_user_map.admin_id = admin_api_keys.admin_id".into(),
    ];
    statements.extend(rebuild_audit_events(dialect));
    statements.extend(rebuild_credential_health(dialect));
    statements.extend([
        "DROP TABLE admin_sessions".into(),
        "DROP TABLE admin_api_keys".into(),
        "DROP TABLE admin_accounts".into(),
        "DROP TABLE wave26_admin_user_map".into(),
    ]);
    statements
}

fn rebuild_audit_events(dialect: Dialect) -> Vec<String> {
    rebuild(
        "admin_audit_events",
        dialect,
        "INSERT INTO admin_audit_events (id, actor_user_id, action, target_kind, target_id, at, details_json) SELECT admin_audit_events_before_wave26.id, wave26_admin_user_map.user_id, admin_audit_events_before_wave26.action, admin_audit_events_before_wave26.target_kind, admin_audit_events_before_wave26.target_id, admin_audit_events_before_wave26.at, admin_audit_events_before_wave26.details_json FROM admin_audit_events_before_wave26 INNER JOIN wave26_admin_user_map ON wave26_admin_user_map.admin_id = admin_audit_events_before_wave26.actor_admin_id",
    )
}

fn rebuild_credential_health(dialect: Dialect) -> Vec<String> {
    rebuild(
        "credential_health",
        dialect,
        "INSERT INTO credential_health (credential_id, model, credential_version, version, state, observed_at, response_status, detail) SELECT credential_id, '*', credential_version, version, state, observed_at, response_status, detail FROM credential_health_before_wave26",
    )
}

fn rebuild(table: &str, dialect: Dialect, copy: &str) -> Vec<String> {
    let spec = tables()
        .find(|candidate| candidate.name == table)
        .expect("wave 26 schema exists");
    let old = format!("{table}_before_wave26");
    let mut statements = vec![
        format!("ALTER TABLE {table} RENAME TO {old}"),
        create_table(spec, SchemaVersion::Wave26, dialect),
        copy.into(),
        format!("DROP TABLE {old}"),
    ];
    statements.extend(
        spec.indexes
            .iter()
            .map(|index| create_index(spec.name, index, dialect)),
    );
    statements
}
