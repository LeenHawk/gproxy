use gproxy_store::entity::{
    config::{connection_profile, setting},
    upstream::{credential, provider},
};
use sea_orm::{DbBackend, EntityTrait, Schema};

fn ddl<E: EntityTrait>(backend: DbBackend, entity: E) -> String {
    backend
        .build(&Schema::new(backend).create_table_from_entity(entity))
        .to_string()
}

#[test]
fn connection_profiles_are_registered_and_references_restrict_deletion() {
    for backend in [DbBackend::Sqlite, DbBackend::Postgres, DbBackend::MySql] {
        let registry = format!("{:?}", gproxy_store::schema(backend));
        assert!(registry.contains("connection_profiles"));
        let profile = ddl(backend, connection_profile::Entity);
        assert!(!profile.contains("proxy_mode"));
        assert!(profile.contains("emulation"));
        assert!(!profile.contains("version"));
        for sql in [
            ddl(backend, setting::Entity),
            ddl(backend, provider::Entity),
            ddl(backend, credential::Entity),
        ] {
            let sql = sql.replace(['"', '`'], "");
            assert!(sql.contains("connection_profile_id"));
            assert!(
                sql.contains("REFERENCES connection_profiles (id) ON DELETE RESTRICT"),
                "{sql}"
            );
            assert!(!sql.contains("inherit_system_proxy"));
        }
    }
}
