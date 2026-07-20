use async_trait::async_trait;

use super::super::{DbPersistence, ops};
use crate::store::persistence::records::{
    Org, OrgInput, Team, TeamInput, User, UserInput, UserKey, UserKeyInput,
};
use crate::store::persistence::traits::IdentityPersistence;

#[async_trait]
impl IdentityPersistence for DbPersistence {
    async fn list_orgs(&self) -> anyhow::Result<Vec<Org>> {
        ops::identity::orgs::list(&self.conn).await
    }
    async fn get_org(&self, id: i64) -> anyhow::Result<Option<Org>> {
        ops::identity::orgs::get(&self.conn, id).await
    }
    async fn get_org_by_name(&self, name: &str) -> anyhow::Result<Option<Org>> {
        ops::identity::orgs::get_by_name(&self.conn, name).await
    }
    async fn upsert_org(&self, input: OrgInput) -> anyhow::Result<Org> {
        ops::identity::orgs::upsert(&self.conn, input).await
    }
    async fn delete_org(&self, id: i64) -> anyhow::Result<bool> {
        ops::identity::orgs::delete(&self.conn, id).await
    }

    async fn list_teams(&self, org_id: i64) -> anyhow::Result<Vec<Team>> {
        ops::identity::teams::list(&self.conn, org_id).await
    }
    async fn get_team(&self, id: i64) -> anyhow::Result<Option<Team>> {
        ops::identity::teams::get(&self.conn, id).await
    }
    async fn upsert_team(&self, input: TeamInput) -> anyhow::Result<Team> {
        ops::identity::teams::upsert(&self.conn, input).await
    }
    async fn delete_team(&self, id: i64) -> anyhow::Result<bool> {
        ops::identity::teams::delete(&self.conn, id).await
    }

    async fn list_users(&self) -> anyhow::Result<Vec<User>> {
        ops::identity::users::list(&self.conn).await
    }
    async fn get_user(&self, id: i64) -> anyhow::Result<Option<User>> {
        ops::identity::users::get(&self.conn, id).await
    }
    async fn get_user_by_name(&self, name: &str) -> anyhow::Result<Option<User>> {
        ops::identity::users::get_by_name(&self.conn, name).await
    }
    async fn upsert_user(&self, input: UserInput) -> anyhow::Result<User> {
        ops::identity::users::upsert(&self.conn, input).await
    }
    async fn delete_user(&self, id: i64) -> anyhow::Result<bool> {
        ops::identity::users::delete(&self.conn, id).await
    }

    async fn list_user_keys(&self, user_id: i64) -> anyhow::Result<Vec<UserKey>> {
        ops::identity::user_keys::list(&self.conn, user_id).await
    }
    async fn get_user_key(&self, id: i64) -> anyhow::Result<Option<UserKey>> {
        ops::identity::user_keys::get(&self.conn, id).await
    }
    async fn find_user_key_by_digest(&self, digest: &str) -> anyhow::Result<Option<UserKey>> {
        ops::identity::user_keys::find_by_digest(&self.conn, digest).await
    }
    async fn upsert_user_key(&self, input: UserKeyInput) -> anyhow::Result<UserKey> {
        ops::identity::user_keys::upsert(&self.conn, input).await
    }
    async fn delete_user_key(&self, id: i64) -> anyhow::Result<bool> {
        ops::identity::user_keys::delete(&self.conn, id).await
    }
}
