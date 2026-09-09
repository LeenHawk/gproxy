use gproxy_store::records::{ProviderInput, RouteInput, RouteMemberInput};
use tokio_rusqlite::rusqlite::{Connection, Result};

use super::super::model::{Alias, Credential, Legacy, ProviderModel, SourceData};
use super::{json, optional_json};

pub(super) fn read(connection: &Connection, data: &mut SourceData) -> Result<()> {
    data.providers = providers(connection)?;
    data.credentials = credentials(connection)?;
    data.routes = routes(connection, &mut data.notices)?;
    data.route_members = route_members(connection)?;
    data.aliases = aliases(connection)?;
    data.provider_models = provider_models(connection)?;
    super::pricing::read(connection, data)?;
    Ok(())
}

fn provider_models(connection: &Connection) -> Result<Vec<Legacy<ProviderModel>>> {
    let mut query = connection.prepare(
        "SELECT id,provider_id,model_id,display_name,variants_json,context_window,max_output_tokens,thinking_supported,thinking_adaptive_supported,thinking_enabled_supported,enabled FROM provider_models ORDER BY id",
    )?;
    query
        .query_map([], |row| {
            Ok(Legacy {
                id: row.get(0)?,
                value: ProviderModel {
                    provider_id: row.get(1)?,
                    model_id: row.get(2)?,
                    display_name: row.get(3)?,
                    variants: optional_json(row, 4)?,
                    context_window: row.get(5)?,
                    max_output_tokens: row.get(6)?,
                    thinking_supported: row.get(7)?,
                    thinking_adaptive_supported: row.get(8)?,
                    thinking_enabled_supported: row.get(9)?,
                    enabled: row.get(10)?,
                },
            })
        })?
        .collect()
}

fn providers(connection: &Connection) -> Result<Vec<Legacy<ProviderInput>>> {
    let mut query = connection.prepare(
        "SELECT id,name,channel,label,settings_json,credential_strategy,proxy_url,tls_fingerprint,enabled FROM providers ORDER BY id",
    )?;
    query
        .query_map([], |row| {
            let id: i64 = row.get(0)?;
            let name: String = row.get(1)?;
            let channel: String = row.get(2)?;
            let settings = json(row, 4)?;
            let settings = gproxy_channels::canonical_provider_settings(&channel, &settings)
                .map_err(|message| {
                    conversion(4, format!("providers id={id} name={name:?}: {message}"))
                })?;
            Ok(Legacy {
                id,
                value: ProviderInput {
                    name,
                    label: row.get(3)?,
                    channel: gproxy_channels::canonical_channel_id(&channel).into(),
                    settings,
                    credential_strategy: row.get(5)?,
                    proxy_url: row.get(6)?,
                    tls_fingerprint: optional_json(row, 7)?,
                    enabled: row.get(8)?,
                },
            })
        })?
        .collect()
}

fn credentials(connection: &Connection) -> Result<Vec<Legacy<Credential>>> {
    let mut query = connection.prepare(
        "SELECT id,provider_id,name,kind,secret_json,weight,rpm_limit,tpm_limit,proxy_url,tls_fingerprint,enabled FROM credentials ORDER BY id",
    )?;
    query
        .query_map([], |row| {
            Ok(Legacy {
                id: row.get(0)?,
                value: Credential {
                    provider_id: row.get(1)?,
                    label: row.get(2)?,
                    kind: row.get(3)?,
                    stored_secret: json(row, 4)?,
                    weight: row.get(5)?,
                    rpm_limit: row.get(6)?,
                    tpm_limit: row.get(7)?,
                    proxy_url: row.get(8)?,
                    tls_fingerprint: optional_json(row, 9)?,
                    enabled: row.get(10)?,
                },
            })
        })?
        .collect()
}

fn routes(
    connection: &Connection,
    notices: &mut Vec<super::super::report::ImportIssue>,
) -> Result<Vec<Legacy<RouteInput>>> {
    let has_strategy: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM pragma_table_info('routes') WHERE name='strategy')",
        [],
        |row| row.get(0),
    )?;
    let strategy = if has_strategy {
        "strategy"
    } else {
        "'round_robin'"
    };
    let mut query = connection.prepare(&format!(
        "SELECT id,name,enabled,{strategy} FROM routes ORDER BY id"
    ))?;
    query
        .query_map([], |row| {
            Ok(Legacy {
                id: row.get(0)?,
                value: RouteInput {
                    name: row.get(1)?,
                    strategy: super::super::compat::route_strategy(
                        &row.get::<_, String>(3)?,
                        row.get(0)?,
                        &row.get::<_, String>(1)?,
                        notices,
                    ),
                    max_attempts: 6,
                    enabled: row.get(2)?,
                },
            })
        })?
        .collect()
}

fn route_members(connection: &Connection) -> Result<Vec<Legacy<RouteMemberInput>>> {
    let mut query = connection.prepare(
        "SELECT id,route_id,provider_id,upstream_model_id,tier,weight,enabled FROM route_members ORDER BY id",
    )?;
    query
        .query_map([], |row| {
            Ok(Legacy {
                id: row.get(0)?,
                value: RouteMemberInput {
                    route_id: row.get(1)?,
                    provider_id: row.get(2)?,
                    upstream_model: row.get(3)?,
                    tier: integer(row, 4)?,
                    weight: integer(row, 5)?,
                    enabled: row.get(6)?,
                },
            })
        })?
        .collect()
}

fn aliases(connection: &Connection) -> Result<Vec<Legacy<Alias>>> {
    let mut query = connection
        .prepare("SELECT id,provider,alias,target,sort_order,enabled FROM aliases ORDER BY id")?;
    query
        .query_map([], |row| {
            Ok(Legacy {
                id: row.get(0)?,
                value: Alias {
                    provider: row.get(1)?,
                    alias: row.get(2)?,
                    target: row.get(3)?,
                    sort_order: row.get(4)?,
                    enabled: row.get(5)?,
                },
            })
        })?
        .collect()
}

fn integer(row: &tokio_rusqlite::rusqlite::Row<'_>, index: usize) -> Result<u32> {
    u32::try_from(row.get::<_, i64>(index)?).map_err(|error| conversion(index, error))
}

fn conversion(index: usize, error: impl std::fmt::Display) -> tokio_rusqlite::rusqlite::Error {
    tokio_rusqlite::rusqlite::Error::FromSqlConversionFailure(
        index,
        tokio_rusqlite::rusqlite::types::Type::Integer,
        Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            error.to_string(),
        )),
    )
}
