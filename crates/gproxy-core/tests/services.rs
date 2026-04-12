//! Integration tests for gproxy-core domain services.

use std::collections::HashMap;

use gproxy_core::{
    FileService, IdentityService, MemoryModel, MemoryUser, MemoryUserCredentialFile, MemoryUserKey,
    PermissionEntry, PolicyService, RoutingService,
};

#[test]
fn identity_service_authenticates_valid_key() {
    let svc = IdentityService::new();
    svc.replace_users(vec![MemoryUser {
        id: 1,
        name: "alice".into(),
        enabled: true,
        is_admin: false,
        password_hash: String::new(),
    }]);
    svc.replace_keys(vec![MemoryUserKey {
        id: 10,
        user_id: 1,
        api_key: "sk-test-123".into(),
        label: None,
        enabled: true,
    }]);

    let result = svc.authenticate_api_key("sk-test-123");
    assert!(result.is_some());
    assert_eq!(result.unwrap().user_id, 1);
}

#[test]
fn identity_service_rejects_disabled_user() {
    let svc = IdentityService::new();
    svc.replace_users(vec![MemoryUser {
        id: 1,
        name: "alice".into(),
        enabled: false,
        is_admin: false,
        password_hash: String::new(),
    }]);
    svc.replace_keys(vec![MemoryUserKey {
        id: 10,
        user_id: 1,
        api_key: "sk-test-123".into(),
        label: None,
        enabled: true,
    }]);

    assert!(svc.authenticate_api_key("sk-test-123").is_none());
}

#[test]
fn identity_service_rejects_disabled_key() {
    let svc = IdentityService::new();
    svc.replace_users(vec![MemoryUser {
        id: 1,
        name: "alice".into(),
        enabled: true,
        is_admin: false,
        password_hash: String::new(),
    }]);
    svc.replace_keys(vec![MemoryUserKey {
        id: 10,
        user_id: 1,
        api_key: "sk-test-123".into(),
        label: None,
        enabled: false,
    }]);

    assert!(svc.authenticate_api_key("sk-test-123").is_none());
}

#[test]
fn policy_service_checks_model_permission() {
    let svc = PolicyService::new();
    let mut perms = HashMap::new();
    perms.insert(
        1i64,
        vec![PermissionEntry {
            id: 1,
            provider_id: Some(100),
            model_pattern: "gpt-*".into(),
        }],
    );
    svc.replace_permissions(perms);

    assert!(svc.check_model_permission(1, 100, "gpt-4o"));
    assert!(!svc.check_model_permission(1, 100, "claude-3"));
    assert!(!svc.check_model_permission(1, 200, "gpt-4o"));
    assert!(!svc.check_model_permission(999, 100, "gpt-4o"));
}

#[test]
fn policy_service_wildcard_permission() {
    let svc = PolicyService::new();
    let mut perms = HashMap::new();
    perms.insert(
        1i64,
        vec![PermissionEntry {
            id: 1,
            provider_id: None,
            model_pattern: "*".into(),
        }],
    );
    svc.replace_permissions(perms);

    assert!(svc.check_model_permission(1, 100, "anything"));
    assert!(svc.check_model_permission(1, 999, "any-model"));
}

#[test]
fn policy_service_suffix_wildcard() {
    let svc = PolicyService::new();
    let mut perms = HashMap::new();
    perms.insert(
        1i64,
        vec![PermissionEntry {
            id: 1,
            provider_id: None,
            model_pattern: "*-turbo".into(),
        }],
    );
    svc.replace_permissions(perms);

    assert!(svc.check_model_permission(1, 100, "gpt-4-turbo"));
    assert!(!svc.check_model_permission(1, 100, "gpt-4o"));
}

#[test]
fn routing_service_resolves_alias() {
    let svc = RoutingService::new();
    // Set up provider names so reverse lookup works
    let mut names = HashMap::new();
    names.insert("openai".into(), 1i64);
    svc.replace_provider_names(names);

    // Add a real model and an alias model pointing to it
    svc.replace_models(vec![
        MemoryModel {
            id: 1,
            provider_id: 1,
            model_id: "gpt-4-turbo".into(),
            display_name: None,
            enabled: true,
            pricing: None,
            alias_of: None,
        },
        MemoryModel {
            id: 2,
            provider_id: 1,
            model_id: "gpt4".into(),
            display_name: None,
            enabled: true,
            pricing: None,
            alias_of: Some(1),
        },
    ]);

    let result = svc.resolve_model_alias("gpt4");
    assert!(result.is_some());
    let target = result.unwrap();
    assert_eq!(target.provider_name, "openai");
    assert_eq!(target.model_id, "gpt-4-turbo");

    assert!(svc.resolve_model_alias("nonexistent").is_none());
}

#[test]
fn routing_service_provider_index_lookups() {
    let svc = RoutingService::new();
    let mut names = HashMap::new();
    names.insert("openai".into(), 1i64);
    names.insert("anthropic".into(), 2i64);
    svc.replace_provider_names(names);

    let mut creds = HashMap::new();
    creds.insert("openai".into(), vec![100i64, 101]);
    svc.replace_provider_credentials(creds);

    assert_eq!(svc.provider_id_for_name("openai"), Some(1));
    assert_eq!(svc.provider_id_for_name("unknown"), None);
    assert_eq!(svc.credential_id_for_index("openai", 0), Some(100));
    assert_eq!(svc.credential_id_for_index("openai", 1), Some(101));
    assert_eq!(svc.credential_id_for_index("openai", 2), None);
}

#[test]
fn file_service_finds_active_file() {
    let svc = FileService::new();
    svc.replace_user_files(vec![
        MemoryUserCredentialFile {
            user_id: 1,
            user_key_id: 10,
            provider_id: 100,
            credential_id: 200,
            file_id: "file-abc".into(),
            active: true,
            created_at_unix_ms: 0,
        },
        MemoryUserCredentialFile {
            user_id: 1,
            user_key_id: 10,
            provider_id: 100,
            credential_id: 200,
            file_id: "file-deleted".into(),
            active: false,
            created_at_unix_ms: 0,
        },
    ]);

    assert!(svc.find_user_file(1, 100, "file-abc").is_some());
    assert!(svc.find_user_file(1, 100, "file-deleted").is_none());
    assert_eq!(svc.list_user_files(1, 100).len(), 1);
}
