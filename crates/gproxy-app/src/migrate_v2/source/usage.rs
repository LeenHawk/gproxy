use tokio_rusqlite::rusqlite::{Connection, Result};

use super::super::model::{Legacy, Settings, SourceData, Usage};
use super::{decimal, json};

pub(super) fn read(connection: &Connection, data: &mut SourceData) -> Result<()> {
    data.settings = settings(connection)?;
    data.usage = usages(connection)?;
    Ok(())
}

fn settings(connection: &Connection) -> Result<Vec<Legacy<Settings>>> {
    let mut query = connection.prepare(
        "SELECT id,instance_name,proxy,enable_usage,enable_upstream_log,enable_upstream_log_body,enable_downstream_log,enable_downstream_log_body,disable_log_redaction,enable_tokenizer_download,retention_days,max_database_size_mb,file_upload_max_in_flight FROM instance_settings ORDER BY id",
    )?;
    query
        .query_map([], |row| {
            Ok(Legacy {
                id: row.get(0)?,
                value: Settings {
                    instance_name: row.get(1)?,
                    proxy: row.get(2)?,
                    enable_usage: row.get(3)?,
                    enable_upstream_log: row.get(4)?,
                    enable_upstream_log_body: row.get(5)?,
                    enable_downstream_log: row.get(6)?,
                    enable_downstream_log_body: row.get(7)?,
                    disable_log_redaction: row.get(8)?,
                    enable_tokenizer_download: row.get(9)?,
                    retention_days: row.get(10)?,
                    max_database_size_mb: row.get(11)?,
                    file_upload_max_in_flight: row.get(12)?,
                },
            })
        })?
        .collect()
}

fn usages(connection: &Connection) -> Result<Vec<Legacy<Usage>>> {
    let mut query = connection.prepare(
        "SELECT id,request_id,at,route_name,provider_id,credential_id,org_id,team_id,user_id,user_key_id,thread_id,operation,kind,model,input_tokens,output_tokens,image_output_tokens,cache_read_tokens,cache_creation_5m_tokens,cache_creation_30m_tokens,cache_creation_1h_tokens,metrics_json,cost,latency_ms,usage_source,ended FROM usages ORDER BY id",
    )?;
    query
        .query_map([], |row| {
            Ok(Legacy {
                id: row.get(0)?,
                value: Usage {
                    request_id: row.get(1)?,
                    at: row.get(2)?,
                    route_name: row.get(3)?,
                    provider_id: row.get(4)?,
                    credential_id: row.get(5)?,
                    organization_id: row.get(6)?,
                    team_id: row.get(7)?,
                    user_id: row.get(8)?,
                    user_key_id: row.get(9)?,
                    thread_id: row.get(10)?,
                    operation: row.get(11)?,
                    kind: row.get(12)?,
                    model: row.get(13)?,
                    input_tokens: row.get(14)?,
                    output_tokens: row.get(15)?,
                    image_output_tokens: row.get(16)?,
                    cache_read_tokens: row.get(17)?,
                    cache_creation_5m_tokens: row.get(18)?,
                    cache_creation_30m_tokens: row.get(19)?,
                    cache_creation_1h_tokens: row.get(20)?,
                    metrics: json(row, 21)?,
                    cost: decimal(row, 22)?,
                    latency_ms: row.get(23)?,
                    usage_source: row.get(24)?,
                    ended: row.get(25)?,
                },
            })
        })?
        .collect()
}
