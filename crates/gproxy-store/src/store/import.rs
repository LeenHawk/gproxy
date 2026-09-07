use crate::backend::Statement;
use crate::query::{control, identity, runtime, usage};
use crate::records::RecordBatch;
use crate::{Store, StoreError};

impl Store {
    pub async fn entity_counts(&self) -> Result<Vec<(&'static str, u64)>, StoreError> {
        let tables = crate::schema::tables().collect::<Vec<_>>();
        let statements = tables
            .iter()
            .map(|table| crate::query::count_all(table.name))
            .collect::<Result<Vec<_>, _>>()?;
        let results = self.backend().batch(statements).await?;
        tables
            .into_iter()
            .zip(results)
            .map(|(table, mut result)| {
                let row = result.rows.pop().ok_or_else(|| {
                    StoreError::Database(format!("{} count row missing", table.name))
                })?;
                let count = u64::try_from(row.i64("count")?).map_err(|_| {
                    StoreError::Database(format!("{} count is negative", table.name))
                })?;
                Ok((table.name, count))
            })
            .collect()
    }

    pub async fn insert_record_batch(&self, batch: RecordBatch) -> Result<Vec<i64>, StoreError> {
        let statements = statements(batch)?;
        if statements.is_empty() {
            return Ok(Vec::new());
        }
        self.backend()
            .batch(statements)
            .await?
            .into_iter()
            .map(|result| {
                result.last_insert_id.ok_or_else(|| {
                    StoreError::Database("batch insert did not return a row id".into())
                })
            })
            .collect()
    }

    pub async fn import_settings(
        &self,
        inputs: &[crate::records::SettingInput],
    ) -> Result<(), StoreError> {
        let statements = inputs
            .iter()
            .map(control::insert_setting)
            .collect::<Result<Vec<_>, _>>()?;
        if !statements.is_empty() {
            self.backend().batch(statements).await?;
        }
        Ok(())
    }

    pub async fn import_usage(
        &self,
        inputs: &[crate::records::UsageInput],
    ) -> Result<u64, StoreError> {
        let mut statements = Vec::with_capacity(inputs.len() * 2);
        for input in inputs {
            statements.push(usage::insert_usage(input)?);
            statements.push(usage::accumulate_hourly(input)?);
        }
        if statements.is_empty() {
            return Ok(0);
        }
        let results = self.backend().batch(statements).await?;
        Ok(results
            .iter()
            .step_by(2)
            .filter(|result| result.affected_rows == 1)
            .count() as u64)
    }
}

fn statements(batch: RecordBatch) -> Result<Vec<Statement>, StoreError> {
    macro_rules! build {
        ($values:expr, $function:path) => {
            $values.iter().map($function).collect()
        };
    }
    match batch {
        RecordBatch::RequestLogs(values) => build!(values, runtime::import_request_log),
        RecordBatch::Captures(values) => build!(values, runtime::insert_capture),
        RecordBatch::Organizations(values) => build!(values, identity::insert_organization),
        RecordBatch::Teams(values) => build!(values, identity::insert_team),
        RecordBatch::Users(values) => build!(values, identity::insert_user),
        RecordBatch::UserKeys(values) => build!(values, identity::insert_user_key),
        RecordBatch::Providers(values) => build!(values, control::insert_provider),
        RecordBatch::Credentials(values) => build!(values, control::insert_credential),
        RecordBatch::Routes(values) => build!(values, control::insert_route),
        RecordBatch::RouteMembers(values) => build!(values, control::insert_route_member),
        RecordBatch::Aliases(values) => build!(values, control::insert_alias),
        RecordBatch::ExposedModels(values) => build!(values, control::insert_exposed_model),
        RecordBatch::ProviderModels(values) => build!(values, control::insert_provider_model),
        RecordBatch::Quotas(values) => build!(values, identity::insert_quota),
        RecordBatch::PriceRules(values) => build!(values, control::insert_price_rule),
        RecordBatch::PriceRates(values) => build!(values, control::insert_price_rate),
        RecordBatch::RoutingRules(values) => build!(values, control::insert_routing_rule),
        RecordBatch::RuleSets(values) => build!(values, control::insert_rule_set),
        RecordBatch::Rules(values) => build!(values, control::insert_rule),
        RecordBatch::ProviderRuleSets(values) => build!(values, control::insert_provider_rule_set),
    }
}
