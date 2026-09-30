//! Database-paged merged catalog. Only the bundled reference data is passed in
//! full; persisted models and provider associations stay inside the database.
use gproxy_seaorm::{BatchConnectionTrait, BatchQuery, D1Type, Projection};
use sea_orm::{DbBackend, Statement};
use serde_json::{Value, json};
use std::collections::HashMap;

use super::catalog::{Catalog, default_metadata};
use crate::{
    SdkError, SdkResult,
    dto::{CatalogModelDto, CatalogProviderDto, ListQuery, ModelDto, Page},
};

impl<C: BatchConnectionTrait + Send + Sync + 'static> Catalog<'_, C> {
    pub async fn models_page(&self, query: ListQuery) -> SdkResult<Page<CatalogModelDto>> {
        let catalog = self.default_models()?;
        let bundled = Value::Array(
            catalog
                .models
                .iter()
                .map(|model| {
                    json!({
                        "name": model.model_id, "metadata": default_metadata(&model.model_id)
                    })
                })
                .collect(),
        )
        .to_string();
        let backend = self.writer.backend();
        let sql = catalog_sql(backend);
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
        let values: Vec<sea_orm::Value> = vec![bundled.into(), search.into()];
        let count = BatchQuery::new(
            Statement::from_sql_and_values(
                backend,
                format!("{sql} SELECT COUNT(*) AS total FROM filtered"),
                values.clone(),
            ),
            Projection::new().column("total", D1Type::I64, false)?,
        );
        let limit_param = match backend {
            DbBackend::Postgres => "$3",
            DbBackend::MySql => "?",
            _ => "?3",
        };
        let offset_param = match backend {
            DbBackend::Postgres => "$4",
            DbBackend::MySql => "?",
            _ => "?4",
        };
        let providers = if backend == DbBackend::Postgres {
            "COALESCE((SELECT jsonb_agg(jsonb_build_object('id', id, 'name', provider_name, 'displayName', display_name) ORDER BY provider_name) FROM linked WHERE catalog_name = f.name), '[]'::jsonb)::text"
        } else if backend == DbBackend::MySql {
            "COALESCE((SELECT CAST(JSON_ARRAYAGG(JSON_OBJECT('id', id, 'name', provider_name, 'displayName', display_name)) AS CHAR) FROM linked WHERE catalog_name = f.name), '[]')"
        } else {
            "(SELECT json_group_array(json_object('id', id, 'name', provider_name, 'displayName', display_name)) FROM (SELECT * FROM linked WHERE catalog_name = f.name ORDER BY provider_name))"
        };
        let page_sql = format!(
            "{sql} SELECT f.name, f.local_id, {providers} AS providers, CASE WHEN EXISTS(SELECT 1 FROM price_rules p WHERE p.provider_id IS NULL AND p.model_pattern = COALESCE(f.local_name, f.name)) THEN 1 ELSE 0 END AS local_price FROM filtered f ORDER BY LOWER(f.name), f.name LIMIT {limit_param} OFFSET {offset_param}"
        );
        let mut page_values = values;
        page_values.extend([
            sea_orm::Value::from(limit as i64),
            sea_orm::Value::from(offset.min(i64::MAX as u64) as i64),
        ]);
        let page = BatchQuery::new(
            Statement::from_sql_and_values(backend, page_sql, page_values),
            Projection::new()
                .column("name", D1Type::Text, false)?
                .column("local_id", D1Type::Text, true)?
                .column("providers", D1Type::Text, false)?
                .column("local_price", D1Type::I32, false)?,
        );
        let mut result = self
            .writer
            .store()
            .connection()
            .query_batch(&[count, page])
            .await?
            .into_iter();
        let count = result.next().unwrap();
        let total: i64 = count[0].try_get("", "total")?;
        let page = result.next().unwrap();
        let ids = page
            .iter()
            .map(|row| row.try_get::<Option<String>>("", "local_id"))
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        let local: HashMap<_, _> = self
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
            let name: String = row.try_get("", "name")?;
            let id: Option<String> = row.try_get("", "local_id")?;
            let local = id.and_then(|id| local.get(&id).cloned());
            let defaults = catalog
                .models
                .iter()
                .find(|model| model.model_id == name)
                .cloned();
            let mut metadata = if defaults.is_some() {
                default_metadata(&name)
            } else {
                json!({})
            };
            if let Some(Value::Object(overrides)) = local.as_ref().map(|row| &row.metadata) {
                metadata.as_object_mut().unwrap().extend(overrides.clone());
            }
            let mut providers: Vec<CatalogProviderDto> =
                serde_json::from_str(&row.try_get::<String>("", "providers")?)
                    .map_err(|error| SdkError::invalid(error.to_string()))?;
            providers.sort_by(|a, b| a.name.cmp(&b.name));
            let price_pattern = if row.try_get::<i32>("", "local_price")? != 0 {
                local.as_ref().map_or(&name, |row| &row.name).clone()
            } else {
                defaults
                    .as_ref()
                    .and_then(|row| row.pricing.as_ref())
                    .map_or(&name, |price| &price.model_pattern)
                    .clone()
            };
            items.push(CatalogModelDto {
                name,
                metadata,
                providers,
                defaults,
                local,
                price_pattern,
            });
        }
        Ok(Page {
            items,
            total: total as u64,
            offset,
            limit,
        })
    }
}

fn catalog_sql(backend: DbBackend) -> String {
    let pg = backend == DbBackend::Postgres;
    let mysql = backend == DbBackend::MySql;
    let args = if pg {
        "SELECT $1::text AS bundled, $2::text AS pattern"
    } else {
        "SELECT ? AS bundled, ? AS pattern"
    };
    let bundle = if pg {
        "SELECT value->>'name' AS name, value->'metadata' AS metadata FROM jsonb_array_elements((SELECT bundled FROM args)::jsonb)"
    } else if mysql {
        "SELECT b.name, b.metadata FROM JSON_TABLE((SELECT bundled FROM args), '$[*]' COLUMNS (name VARCHAR(1024) PATH '$.name', metadata JSON PATH '$.metadata')) AS b"
    } else {
        "SELECT json_extract(value, '$.name') AS name, json_extract(value, '$.metadata') AS metadata FROM json_each((SELECT bundled FROM args))"
    };
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
    let local_base = basename("m.name");
    let row_base = basename("name");
    let binding_base = basename("pm.upstream_name");
    let fields = ["display_name", "input_modalities", "output_modalities", "supported_parameters"].map(|field| {
        if pg { format!("COALESCE((CASE WHEN c.local_metadata::jsonb ? '{field}' THEN c.local_metadata::jsonb ELSE c.default_metadata END)->>'{field}', '')") }
        else if mysql { format!("COALESCE(NULLIF(CASE WHEN JSON_CONTAINS_PATH(c.local_metadata, 'one', '$.{field}') THEN JSON_UNQUOTE(JSON_EXTRACT(c.local_metadata, '$.{field}')) ELSE JSON_UNQUOTE(JSON_EXTRACT(c.default_metadata, '$.{field}')) END, 'null'), '')") }
        else { format!("COALESCE(CASE WHEN json_type(c.local_metadata, '$.{field}') IS NOT NULL THEN json_extract(c.local_metadata, '$.{field}') ELSE json_extract(c.default_metadata, '$.{field}') END, '')") }
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
    format!(
        r#"WITH args AS ({args}), bundled AS ({bundle}),
    local_names AS (SELECT m.id, LOWER(m.name) AS exact_name, {local_base} AS basename FROM models m),
    default_matches AS (
      SELECT b.name, b.metadata, COALESCE(
        MIN(CASE WHEN m.exact_name = LOWER(b.name) THEN m.id END),
        CASE WHEN COUNT(m.id) = 1 THEN MIN(m.id) END
      ) AS local_id FROM bundled b LEFT JOIN local_names m ON m.basename = LOWER(b.name)
      GROUP BY b.name, b.metadata
    ), catalog AS (
      SELECT d.name, d.metadata AS default_metadata, m.id AS local_id, m.name AS local_name, m.metadata AS local_metadata
        FROM default_matches d LEFT JOIN models m ON m.id = d.local_id
      UNION ALL
      SELECT m.name, NULL, m.id, m.name, m.metadata FROM models m
        WHERE NOT EXISTS (SELECT 1 FROM default_matches d WHERE d.local_id = m.id)
    ), names AS (
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
