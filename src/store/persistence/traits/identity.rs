use crate::store::persistence::records::{
    Org, OrgInput, Team, TeamInput, User, UserInput, UserKey, UserKeyInput,
};

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
pub trait IdentityPersistence {
    async fn list_orgs(&self) -> anyhow::Result<Vec<Org>>;
    async fn get_org(&self, id: i64) -> anyhow::Result<Option<Org>>;
    async fn get_org_by_name(&self, name: &str) -> anyhow::Result<Option<Org>>;
    async fn upsert_org(&self, input: OrgInput) -> anyhow::Result<Org>;
    async fn delete_org(&self, id: i64) -> anyhow::Result<bool>;

    async fn list_teams(&self, org_id: i64) -> anyhow::Result<Vec<Team>>;
    async fn get_team(&self, id: i64) -> anyhow::Result<Option<Team>>;
    async fn upsert_team(&self, input: TeamInput) -> anyhow::Result<Team>;
    async fn delete_team(&self, id: i64) -> anyhow::Result<bool>;

    async fn list_users(&self) -> anyhow::Result<Vec<User>>;
    async fn get_user(&self, id: i64) -> anyhow::Result<Option<User>>;
    async fn get_user_by_name(&self, name: &str) -> anyhow::Result<Option<User>>;
    async fn upsert_user(&self, input: UserInput) -> anyhow::Result<User>;
    async fn delete_user(&self, id: i64) -> anyhow::Result<bool>;

    async fn list_user_keys(&self, user_id: i64) -> anyhow::Result<Vec<UserKey>>;
    async fn get_user_key(&self, id: i64) -> anyhow::Result<Option<UserKey>>;
    async fn find_user_key_by_digest(&self, digest: &str) -> anyhow::Result<Option<UserKey>>;
    async fn upsert_user_key(&self, input: UserKeyInput) -> anyhow::Result<UserKey>;
    async fn delete_user_key(&self, id: i64) -> anyhow::Result<bool>;
}
