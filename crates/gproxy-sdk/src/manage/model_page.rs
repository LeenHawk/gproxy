//! Database-paged model records and their saved provider associations.
use super::{catalog::Catalog, models::Models};
use crate::{
    SdkError, SdkResult,
    dto::{CatalogProviderDto, ListQuery, ModelDto, Page},
};
use gproxy_seaorm::{BatchConnectionTrait, BatchQuery, D1Type, Projection};
use sea_orm::{DbBackend, Statement};
use std::collections::HashMap;

impl<C: BatchConnectionTrait + Send + Sync + 'static> Models<'_, C> {
    pub(super) async fn page(&self, query: ListQuery) -> SdkResult<Page<ModelDto>> {
        let backend = self.writer.backend();
        let sql = models_sql(backend);
        let search = query
            .search
            .clone()
            .unwrap_or_default()
            .trim()
            .to_lowercase();
        let search = format!(
            "%{}%",
            search
                .replace('!', "!!")
                .replace('%', "!%")
                .replace('_', "!_")
        );
        let (offset, limit) = query.bounds();
        let values: Vec<sea_orm::Value> = vec![search.into()];
        let count = BatchQuery::new(
            Statement::from_sql_and_values(
                backend,
                format!("{sql} SELECT COUNT(*) AS total FROM filtered"),
                values.clone(),
            ),
            Projection::new().column("total", D1Type::I64, false)?,
        );
        let (limit_param, offset_param) = match backend {
            DbBackend::Postgres => ("$2", "$3"),
            DbBackend::MySql => ("?", "?"),
            _ => ("?2", "?3"),
        };
        let providers = if backend == DbBackend::Postgres {
            "COALESCE((SELECT jsonb_agg(jsonb_build_object('id', id, 'name', provider_name, 'displayName', display_name) ORDER BY provider_name) FROM linked WHERE catalog_name = f.name), '[]'::jsonb)::text"
        } else if backend == DbBackend::MySql {
            "COALESCE((SELECT CAST(JSON_ARRAYAGG(JSON_OBJECT('id', id, 'name', provider_name, 'displayName', display_name)) AS CHAR) FROM linked WHERE catalog_name = f.name), '[]')"
        } else {
            "(SELECT json_group_array(json_object('id', id, 'name', provider_name, 'displayName', display_name)) FROM (SELECT * FROM linked WHERE catalog_name = f.name ORDER BY provider_name))"
        };
        let mut page_values = values;
        page_values.extend([
            sea_orm::Value::from(limit as i64),
            sea_orm::Value::from(offset.min(i64::MAX as u64) as i64),
        ]);
        let page = BatchQuery::new(
            Statement::from_sql_and_values(
                backend,
                format!(
                    "{sql} SELECT f.local_id, {providers} AS providers FROM filtered f ORDER BY LOWER(f.name), f.name LIMIT {limit_param} OFFSET {offset_param}"
                ),
                page_values,
            ),
            Projection::new()
                .column("local_id", D1Type::Text, false)?
                .column("providers", D1Type::Text, false)?,
        );
        let mut result = self
            .writer
            .store()
            .connection()
            .query_batch(&[count, page])
            .await?
            .into_iter();
        let total: i64 = result.next().unwrap()[0].try_get("", "total")?;
        let page = result.next().unwrap();
        let ids = page
            .iter()
            .map(|row| row.try_get::<String>("", "local_id"))
            .collect::<Result<Vec<_>, _>>()?;
        let mut records: HashMap<_, _> = self
            .writer
            .store()
            .models()
            .get_many(&ids)
            .await?
            .into_iter()
            .flatten()
            .map(|row| (row.id.clone(), ModelDto::from(row)))
            .collect();
        let mut items = Vec::with_capacity(page.len());
        for row in page {
            let id: String = row.try_get("", "local_id")?;
            if let Some(mut model) = records.remove(&id) {
                let mut providers: Vec<CatalogProviderDto> =
                    serde_json::from_str(&row.try_get::<String>("", "providers")?)
                        .map_err(|error| SdkError::invalid(error.to_string()))?;
                providers.sort_by(|a, b| a.name.cmp(&b.name));
                model.providers = providers;
                items.push(model);
            }
        }
        Ok(Page {
            items,
            total: total as u64,
            offset,
            limit,
        })
    }
}

fn models_sql(backend: DbBackend) -> String {
    let pg = backend == DbBackend::Postgres;
    let mysql = backend == DbBackend::MySql;
    let basename = |column: &str| {
        if pg {
            format!("LOWER(regexp_replace(TRIM({column}), '^.*/', ''))")
        } else if mysql {
            format!("LOWER(SUBSTRING_INDEX(TRIM({column}), '/', -1))")
        } else {
            format!(
                "LOWER(json_extract('[' || replace(json_quote(TRIM({column})), '/', '\",\"') || ']', '$[#-1]'))"
            )
        }
    };
    let row_base = basename("name");
    let binding_base = basename("pm.upstream_name");
    let fields = [
        "display_name",
        "input_modalities",
        "output_modalities",
        "supported_parameters",
    ]
    .map(|field| {
        if pg {
            format!("COALESCE(c.metadata->>'{field}', '')")
        } else if mysql {
            format!("COALESCE(JSON_UNQUOTE(JSON_EXTRACT(c.metadata, '$.{field}')), '')")
        } else {
            format!("COALESCE(json_extract(c.metadata, '$.{field}'), '')")
        }
    });
    let fields = if mysql {
        format!("CONCAT_WS(' ', c.name, {})", fields.join(", "))
    } else {
        format!("c.name || ' ' || {}", fields.join(" || ' ' || "))
    };
    let provider_text = if mysql {
        "CONCAT_WS(' ', l.provider_name, l.display_name)"
    } else {
        "l.provider_name || ' ' || COALESCE(l.display_name, '')"
    };
    let search = "(SELECT pattern FROM args)";
    let parameter = if pg { "$1" } else { "?" };
    format!(
        r#"WITH args AS (SELECT {parameter} AS pattern), catalog AS (SELECT id AS local_id, name, metadata FROM models),
    names AS (
      SELECT name, local_id, LOWER(name) AS exact_name, {row_base} AS basename FROM catalog
    ), name_counts AS (SELECT basename, COUNT(*) AS total FROM names GROUP BY basename),
    exact_names AS (SELECT exact_name, MIN(name) AS name FROM names GROUP BY exact_name),
    linked AS (
      SELECT DISTINCT n.name AS catalog_name, p.id, p.name AS provider_name, p.display_name
      FROM provider_models pm JOIN providers p ON p.id = pm.provider_id
      LEFT JOIN exact_names e ON e.exact_name = LOWER(pm.upstream_name)
      LEFT JOIN name_counts nc ON nc.basename = {binding_base}
      JOIN names n ON (pm.model_id IS NOT NULL AND n.local_id = pm.model_id) OR
        (pm.model_id IS NULL AND (n.name = e.name OR (e.name IS NULL AND n.basename = nc.basename AND nc.total = 1)))
    ), filtered AS (
      SELECT c.* FROM catalog c WHERE LOWER({fields}) LIKE {search} ESCAPE '!'
      OR EXISTS (SELECT 1 FROM linked l WHERE l.catalog_name = c.name AND LOWER({provider_text}) LIKE {search} ESCAPE '!')
    )"#
    )
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> Catalog<'_, C> {
    /// Saved provider model and variant names for rewrite matchers.
    /// A provider takes precedence over the providers attached to a rule set.
    pub async fn model_names(&self, query: ListQuery) -> SdkResult<Page<String>> {
        let backend = self.writer.backend();
        let (offset, limit) = query.bounds();
        let needle = query.search.unwrap_or_default().trim().to_lowercase();
        let needle = format!(
            "%{}%",
            needle
                .replace('!', "!!")
                .replace('%', "!%")
                .replace('_', "!_")
        );
        let parameter = |i| {
            if backend == DbBackend::Postgres {
                format!("${i}")
            } else {
                "?".into()
            }
        };
        let mut values: Vec<sea_orm::Value> = Vec::new();
        let scope = if let Some(provider_id) = query.provider_id {
            values.push(provider_id.into());
            format!("provider_id = {}", parameter(1))
        } else if let Some(rule_set_id) = query.rule_set_id {
            values.push(rule_set_id.into());
            format!(
                "provider_id IN (SELECT provider_id FROM provider_rewrite_rule_sets WHERE rule_set_id = {})",
                parameter(1)
            )
        } else {
            "TRUE".to_owned()
        };
        let variants = match backend {
            DbBackend::Postgres => {
                "SELECT jsonb_array_elements_text(metadata::jsonb->'variants') AS name FROM saved"
            }
            DbBackend::MySql => {
                "SELECT v.name FROM saved s JOIN JSON_TABLE(s.metadata, '$.variants[*]' COLUMNS (name VARCHAR(1024) PATH '$')) AS v ON TRUE"
            }
            _ => "SELECT v.value AS name FROM saved s, json_each(s.metadata, '$.variants') v",
        };
        values.push(needle.into());
        let sql = format!(
            "WITH saved AS (SELECT upstream_name, metadata FROM provider_models WHERE {scope}), names AS (SELECT upstream_name AS name FROM saved UNION {variants}) SELECT name FROM names WHERE LOWER(name) LIKE {} ESCAPE '!'",
            parameter(values.len())
        );
        let count = BatchQuery::new(
            Statement::from_sql_and_values(
                backend,
                format!("SELECT COUNT(*) AS total FROM ({sql}) matched"),
                values.clone(),
            ),
            Projection::new().column("total", D1Type::I64, false)?,
        );
        let page_sql = format!(
            "{sql} ORDER BY name LIMIT {} OFFSET {}",
            parameter(values.len() + 1),
            parameter(values.len() + 2)
        );
        values.extend([
            (limit as i64).into(),
            (offset.min(i64::MAX as u64) as i64).into(),
        ]);
        let page = BatchQuery::new(
            Statement::from_sql_and_values(backend, page_sql, values),
            Projection::new().column("name", D1Type::Text, false)?,
        );
        let mut result = self
            .writer
            .store()
            .connection()
            .query_batch(&[count, page])
            .await?
            .into_iter();
        let total: i64 = result.next().unwrap()[0].try_get("", "total")?;
        let items = result
            .next()
            .unwrap()
            .iter()
            .map(|row| row.try_get("", "name"))
            .collect::<Result<_, _>>()?;
        Ok(Page {
            items,
            total: total as u64,
            offset,
            limit,
        })
    }
}
