use crate::store::persistence::records::{
    Org, OrgInput, Team, TeamInput, User, UserInput, UserKey, UserKeyInput,
};
use crate::store::persistence::traits::IdentityPersistence;

use super::super::{LibsqlPersistence, identity};

#[async_trait::async_trait(?Send)]
impl IdentityPersistence for LibsqlPersistence {
    async fn list_orgs(&self) -> anyhow::Result<Vec<Org>> {
        identity::orgs::list(&self.client).await
    }
    async fn get_org(&self, id: i64) -> anyhow::Result<Option<Org>> {
        identity::orgs::get(&self.client, id).await
    }
    async fn get_org_by_name(&self, name: &str) -> anyhow::Result<Option<Org>> {
        identity::orgs::get_by_name(&self.client, name).await
    }
    async fn upsert_org(&self, input: OrgInput) -> anyhow::Result<Org> {
        identity::orgs::upsert(&self.client, input).await
    }
    async fn delete_org(&self, id: i64) -> anyhow::Result<bool> {
        identity::orgs::delete(&self.client, id).await
    }

    async fn list_teams(&self, org_id: i64) -> anyhow::Result<Vec<Team>> {
        identity::teams::list(&self.client, org_id).await
    }
    async fn get_team(&self, id: i64) -> anyhow::Result<Option<Team>> {
        identity::teams::get(&self.client, id).await
    }
    async fn upsert_team(&self, input: TeamInput) -> anyhow::Result<Team> {
        identity::teams::upsert(&self.client, input).await
    }
    async fn delete_team(&self, id: i64) -> anyhow::Result<bool> {
        identity::teams::delete(&self.client, id).await
    }

    async fn list_users(&self) -> anyhow::Result<Vec<User>> {
        identity::users::list(&self.client).await
    }
    async fn get_user(&self, id: i64) -> anyhow::Result<Option<User>> {
        identity::users::get(&self.client, id).await
    }
    async fn get_user_by_name(&self, name: &str) -> anyhow::Result<Option<User>> {
        identity::users::get_by_name(&self.client, name).await
    }
    async fn upsert_user(&self, input: UserInput) -> anyhow::Result<User> {
        identity::users::upsert(&self.client, input).await
    }
    async fn delete_user(&self, id: i64) -> anyhow::Result<bool> {
        identity::users::delete(&self.client, id).await
    }

    async fn list_user_keys(&self, user_id: i64) -> anyhow::Result<Vec<UserKey>> {
        identity::user_keys::list(&self.client, user_id).await
    }
    async fn get_user_key(&self, id: i64) -> anyhow::Result<Option<UserKey>> {
        identity::user_keys::get(&self.client, id).await
    }
    async fn find_user_key_by_digest(&self, digest: &str) -> anyhow::Result<Option<UserKey>> {
        identity::user_keys::find_by_digest(&self.client, digest).await
    }
    async fn upsert_user_key(&self, input: UserKeyInput) -> anyhow::Result<UserKey> {
        identity::user_keys::upsert(&self.client, input).await
    }
    async fn delete_user_key(&self, id: i64) -> anyhow::Result<bool> {
        identity::user_keys::delete(&self.client, id).await
    }
}
