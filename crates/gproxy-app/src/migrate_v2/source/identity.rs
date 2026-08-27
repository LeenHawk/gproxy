use gproxy_store::records::{OrganizationInput, TeamInput, UserInput};
use tokio_rusqlite::rusqlite::{Connection, Result};

use super::super::model::{Legacy, Quota, SourceData, UserKey};
use super::{decimal, optional_decimal};

pub(super) fn read(connection: &Connection, data: &mut SourceData) -> Result<()> {
    data.organizations = organizations(connection)?;
    data.teams = teams(connection)?;
    data.users = users(connection)?;
    data.user_keys = user_keys(connection)?;
    data.quotas = quotas(connection)?;
    Ok(())
}

fn organizations(connection: &Connection) -> Result<Vec<Legacy<OrganizationInput>>> {
    let mut query = connection.prepare("SELECT id,name,enabled FROM orgs ORDER BY id")?;
    query
        .query_map([], |row| {
            Ok(Legacy {
                id: row.get(0)?,
                value: OrganizationInput {
                    name: row.get(1)?,
                    enabled: row.get(2)?,
                },
            })
        })?
        .collect()
}

fn teams(connection: &Connection) -> Result<Vec<Legacy<TeamInput>>> {
    let mut query = connection.prepare("SELECT id,org_id,name,enabled FROM teams ORDER BY id")?;
    query
        .query_map([], |row| {
            Ok(Legacy {
                id: row.get(0)?,
                value: TeamInput {
                    organization_id: row.get(1)?,
                    name: row.get(2)?,
                    enabled: row.get(3)?,
                },
            })
        })?
        .collect()
}

fn users(connection: &Connection) -> Result<Vec<Legacy<UserInput>>> {
    let mut query =
        connection.prepare("SELECT id,name,org_id,team_id,enabled FROM users ORDER BY id")?;
    query
        .query_map([], |row| {
            Ok(Legacy {
                id: row.get(0)?,
                value: UserInput {
                    name: row.get(1)?,
                    organization_id: row.get(2)?,
                    team_id: row.get(3)?,
                    enabled: row.get(4)?,
                },
            })
        })?
        .collect()
}

fn user_keys(connection: &Connection) -> Result<Vec<Legacy<UserKey>>> {
    let mut query = connection
        .prepare("SELECT id,user_id,api_key_ciphertext,label,enabled FROM user_keys ORDER BY id")?;
    query
        .query_map([], |row| {
            Ok(Legacy {
                id: row.get(0)?,
                value: UserKey {
                    user_id: row.get(1)?,
                    stored_key: row.get(2)?,
                    label: row.get(3)?,
                    enabled: row.get(4)?,
                },
            })
        })?
        .collect()
}

fn quotas(connection: &Connection) -> Result<Vec<Legacy<Quota>>> {
    let mut query = connection.prepare(
        "SELECT id,scope,scope_id,quota_total,quota_daily,quota_weekly,quota_monthly,quota_5h,quota_7d FROM quotas ORDER BY id",
    )?;
    query
        .query_map([], |row| {
            Ok(Legacy {
                id: row.get(0)?,
                value: Quota {
                    scope: row.get(1)?,
                    scope_id: row.get(2)?,
                    quota_total: decimal(row, 3)?,
                    quota_daily: optional_decimal(row, 4)?,
                    quota_weekly: optional_decimal(row, 5)?,
                    quota_monthly: optional_decimal(row, 6)?,
                    quota_5h: optional_decimal(row, 7)?,
                    quota_7d: optional_decimal(row, 8)?,
                },
            })
        })?
        .collect()
}
