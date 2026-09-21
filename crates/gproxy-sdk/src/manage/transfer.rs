//! Configuration transfer: the whole instance as one document, and back.
//!
//! An export is the management families' own DTOs collected from one read, so
//! it says exactly what a console would have listed. What travels is the
//! configuration a deployment is: connection profiles, providers, credentials,
//! the model catalog, routing, operation overrides, rewrite rules, quotas,
//! pricing and the settings row.
//!
//! What deliberately does not travel is everything that is **about** an
//! instance rather than configured on it. Identity (users, API keys,
//! organizations, teams, permissions, subscriptions, OAuth clients) belongs to
//! the application layer and is never in reach of this crate. Usage records,
//! quota windows, counted windows, settlements, credential cycles and blocks,
//! captures, agent sessions, protocol states, cache rows and file objects are
//! observations and runtime state: copying them onto another instance would
//! fabricate history it never had.
//!
//! # Secrets
//!
//! A credential's sealed blob travels byte for byte, base64-encoded, never
//! opened and never plaintext. An export with secrets is therefore exactly as
//! sensitive as the database file it came from. The destination has three
//! ways to end up with a usable credential:
//!
//! | The importer has | What happens |
//! |---|---|
//! | the source master key | every secret is opened and resealed under this instance's codec |
//! | the same codec as the source | the blob is stored verbatim and already opens |
//! | neither | the credential is skipped, counted and warned about |
//!
//! The last row is not pedantry: core opens every credential's secret while it
//! assembles a snapshot, so a single unopenable row would fail every reload of
//! the destination instance. A skipped credential costs one login; an imported
//! unopenable one costs the whole deployment.

use std::collections::{BTreeSet, HashSet};

use base64::Engine;
use gproxy_core::{AesGcmCodec, SecretCodec};
use gproxy_seaorm::{BatchConnectionTrait, BatchStatement};
use gproxy_store::entity::{
    config::{connection_profile as profile, setting},
    limits::quota,
    pricing::{price_rate, price_rule, price_tier},
    routing::{exposed_model, route, route_member},
    upstream::{
        credential, model, operation_endpoint, operation_rule, provider, provider_model,
        provider_rewrite_rule_set, rewrite_rule, rewrite_rule_set,
    },
};
use sea_orm::Set;

use super::{Scope, Writer, crud};
use crate::{
    SdkError, SdkResult,
    dto::{
        CODEC_AES_GCM, CODEC_PLAINTEXT, CODEC_UNKNOWN, ConfigurationDataDto,
        ConfigurationExportDto, ConnectionProfileDto, CredentialDto, EXPORT_FORMAT_VERSION,
        ExportCredentialDto, ExportRequest, ExposedModelDto, ImportMode, ImportReportDto,
        ImportRequest, ModelDto, OperationEndpointDto, OperationRuleDto, PriceRateDto,
        PriceRuleDto, PriceTierDto, ProviderDto, ProviderModelDto, ProviderRuleSetDto, QuotaDto,
        RewriteRuleDto, RouteDto, RouteMemberDto, RuleSetDto, SealedSecretDto, SettingsDto,
    },
};

const BACKENDS: [&str; 3] = ["reqwest", "wreq", "reqwest_native"];
const PROXY_MODES: [&str; 3] = ["direct", "system", "explicit"];
const RETRIES: [&str; 2] = ["never", "default"];
const STATUSES: [&str; 2] = ["active", "dead"];
const STRATEGIES: [&str; 3] = ["round_robin", "weighted", "failover"];
const TARGETS: [&str; 3] = ["body", "header", "query"];
const TRANSPORTS: [&str; 2] = ["http", "websocket"];
const UNITS: [&str; 4] = ["token", "count", "second", "character"];

/// The envelope byte every sealed blob starts with. Reading it is how an
/// export reports what its secrets are without consulting this instance's
/// configuration: what a row *is* matters more than what the host is set to.
const ENVELOPE_PLAINTEXT: u8 = 0x00;
const ENVELOPE_AES_GCM: u8 = 0x01;

const B64: base64::engine::general_purpose::GeneralPurpose =
    base64::engine::general_purpose::STANDARD;

pub struct Transfer<'a, C> {
    writer: Writer<'a, C>,
}

impl<'a, C> Transfer<'a, C> {
    pub(crate) fn new(writer: Writer<'a, C>) -> Self {
        Self { writer }
    }
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> Transfer<'_, C> {
    /// Every configured row of this instance as one document.
    ///
    /// Read from `load_control_data` and `load_routing_data`, which is one
    /// batch each, so the document is a coherent view rather than seventeen
    /// independently-timed reads.
    pub async fn export(&self, request: ExportRequest) -> SdkResult<ConfigurationExportDto> {
        let control = self.writer.store().load_control_data().await?;
        let routing = self.writer.store().load_routing_data().await?;

        let mut codecs: BTreeSet<String> = BTreeSet::new();
        let credentials: Vec<ExportCredentialDto> = control
            .credentials
            .into_iter()
            .map(|row| {
                let secret = match request.include_secrets && !row.secret.is_empty() {
                    true => {
                        let codec = codec_name(&row.secret);
                        codecs.insert(codec.to_owned());
                        Some(SealedSecretDto {
                            codec: codec.to_owned(),
                            bytes: B64.encode(&row.secret),
                        })
                    }
                    false => None,
                };
                ExportCredentialDto {
                    credential: CredentialDto::from(row),
                    secret,
                }
            })
            .collect();

        Ok(ConfigurationExportDto {
            format_version: EXPORT_FORMAT_VERSION,
            exported_at_ms: crate::rt::now_ms(),
            secrets_omitted: !request.include_secrets,
            secrets: codecs.into_iter().collect(),
            data: ConfigurationDataDto {
                connection_profiles: map(control.connection_profiles),
                providers: map(control.providers),
                credentials,
                models: map(control.models),
                provider_models: map(control.provider_models),
                routes: map(routing.routes),
                route_members: map(routing.route_members),
                exposed_models: map(routing.exposed_models),
                operation_rules: map(control.operation_rules),
                operation_endpoints: map(control.operation_endpoints),
                rewrite_rule_sets: map(control.rewrite_rule_sets),
                rewrite_rules: map(control.rewrite_rules),
                provider_rewrite_rule_sets: map(control.provider_rewrite_rule_sets),
                quotas: map(control.quotas),
                price_rules: map(control.price_rules),
                price_rates: map(control.price_rates),
                price_tiers: map(control.price_tiers),
                settings: control.settings.map(SettingsDto::from),
            },
        })
    }

    /// Replay a document into this instance, as one revision commit.
    ///
    /// Every row of every kind, the settings update and — in
    /// [`ImportMode::Replace`] — every deletion are the same transaction as
    /// the revision bump. A document that is refused anywhere leaves nothing
    /// behind, which is the only way an operator can retry an import without
    /// first working out how far the last one got.
    pub async fn import(&self, request: ImportRequest) -> SdkResult<ImportReportDto> {
        let ImportRequest {
            export,
            mode,
            source_master_key,
        } = request;
        if export.format_version != EXPORT_FORMAT_VERSION {
            return Err(SdkError::invalid(format!(
                "unsupported export format version {}; this build reads {EXPORT_FORMAT_VERSION}",
                export.format_version
            )));
        }
        let source_codec = match source_master_key.as_deref() {
            Some(key) => Some(master_key(key)?),
            None => None,
        };

        let data = export.data;
        let current = self.writer.store().load_control_data().await?;
        let current_routing = self.writer.store().load_routing_data().await?;
        let mut report = ImportReportDto::default();

        // Under Replace the destination's own rows are on their way out, so
        // only the document may satisfy a reference. Under Merge both count.
        let merge = matches!(mode, ImportMode::Merge);
        let known = Known {
            profiles: reach(
                ids(&data.connection_profiles, |row| &row.id),
                &current.connection_profiles,
                |row| &row.id,
                merge,
            ),
            providers: reach(
                ids(&data.providers, |row| &row.id),
                &current.providers,
                |row| &row.id,
                merge,
            ),
            models: reach(
                ids(&data.models, |row| &row.id),
                &current.models,
                |row| &row.id,
                merge,
            ),
            routes: reach(
                ids(&data.routes, |row| &row.id),
                &current_routing.routes,
                |row| &row.id,
                merge,
            ),
            rule_sets: reach(
                ids(&data.rewrite_rule_sets, |row| &row.id),
                &current.rewrite_rule_sets,
                |row| &row.id,
                merge,
            ),
            price_rules: reach(
                ids(&data.price_rules, |row| &row.id),
                &current.price_rules,
                |row| &row.id,
                merge,
            ),
        };

        let present = Present {
            profiles: ids(&current.connection_profiles, |row| &row.id),
            providers: ids(&current.providers, |row| &row.id),
            credentials: ids(&current.credentials, |row| &row.id),
            models: ids(&current.models, |row| &row.id),
            provider_models: ids(&current.provider_models, |row| &row.id),
            routes: ids(&current_routing.routes, |row| &row.id),
            route_members: ids(&current_routing.route_members, |row| &row.id),
            exposed_models: ids(&current_routing.exposed_models, |row| &row.id),
            operation_rules: ids(&current.operation_rules, |row| &row.id),
            operation_endpoints: ids(&current.operation_endpoints, |row| &row.id),
            rule_sets: ids(&current.rewrite_rule_sets, |row| &row.id),
            rewrite_rules: ids(&current.rewrite_rules, |row| &row.id),
            provider_rule_sets: ids(&current.provider_rewrite_rule_sets, |row| &row.id),
            quotas: ids(&current.quotas, |row| &row.id),
            price_rules: ids(&current.price_rules, |row| &row.id),
            price_rates: ids(&current.price_rates, |row| &row.id),
            price_tiers: ids(&current.price_tiers, |row| &row.id),
        };

        // Everything referenced outside the exported set has to be checked
        // against this instance alone: file objects are storage, and the three
        // credential owner columns belong to the application layer.
        let files = self.existing_files(&data).await?;
        let owners = self.existing_owners(&data).await?;

        let store = self.writer.store();
        let mut statements: Vec<BatchStatement> = Vec::new();

        // One row's statement, counted as a create or an update. Only the
        // statement that is actually needed is built: a credential that keeps
        // the secret it already has leaves that column unset, which an insert
        // could not express.
        macro_rules! write_row {
            ($repository:expr, $model:expr, $exists:expr) => {{
                let model = $model;
                if $exists {
                    report.updated += 1;
                    if let Some(statement) = $repository.update_statement(model)? {
                        statements.push(BatchStatement::Execute(statement));
                    }
                } else {
                    report.created += 1;
                    statements.push(BatchStatement::Execute(
                        $repository.insert_statement(model)?,
                    ));
                }
            }};
        }

        // Replace first repoints the settings row away from anything it is
        // about to delete; the row's real values are written at the very end,
        // once every profile and file the document names exists.
        if !merge && data.settings.is_some() {
            statements.push(BatchStatement::Execute(store.settings().update_statement(
                setting::ActiveModel {
                    id: Set(setting::GLOBAL_SETTINGS_ID),
                    connection_profile_id: Set(None),
                    default_vocabulary_file_id: Set(None),
                    ..Default::default()
                },
            )?));
        }
        if !merge {
            self.deletions(
                &data,
                &current,
                &current_routing,
                &mut statements,
                &mut report,
            );
        }

        for row in &data.connection_profiles {
            write_row!(
                store.connection_profiles(),
                rows::profile(row)?,
                present.profiles.contains(&row.id)
            );
        }
        for row in &data.providers {
            known.require(
                &known.profiles,
                row.connection_profile_id.as_deref(),
                "provider",
                &row.id,
                "connection profile",
            )?;
            write_row!(
                store.providers(),
                rows::provider(row)?,
                present.providers.contains(&row.id)
            );
        }
        for row in &data.credentials {
            let id = &row.credential.id;
            known.require(
                &known.providers,
                Some(row.credential.provider_id.as_str()),
                "credential",
                id,
                "provider",
            )?;
            known.require(
                &known.profiles,
                row.credential.connection_profile_id.as_deref(),
                "credential",
                id,
                "connection profile",
            )?;
            let exists = present.credentials.contains(id);
            let Some(secret) = self.secret(row, exists, source_codec.as_ref(), &mut report)? else {
                continue;
            };
            write_row!(
                store.credentials(),
                rows::credential(row, secret, &owners, &mut report.warnings)?,
                exists
            );
        }
        for row in &data.models {
            write_row!(
                store.models(),
                rows::model(row, &files, &mut report.warnings),
                present.models.contains(&row.id)
            );
        }
        for row in &data.provider_models {
            known.require(
                &known.providers,
                Some(row.provider_id.as_str()),
                "provider model",
                &row.id,
                "provider",
            )?;
            known.require(
                &known.models,
                row.model_id.as_deref(),
                "provider model",
                &row.id,
                "model",
            )?;
            write_row!(
                store.provider_models(),
                rows::provider_model(row),
                present.provider_models.contains(&row.id)
            );
        }
        for row in &data.routes {
            write_row!(
                store.routes(),
                rows::route(row)?,
                present.routes.contains(&row.id)
            );
        }
        for row in &data.route_members {
            known.require(
                &known.routes,
                Some(row.route_id.as_str()),
                "route member",
                &row.id,
                "route",
            )?;
            known.require(
                &known.providers,
                Some(row.provider_id.as_str()),
                "route member",
                &row.id,
                "provider",
            )?;
            write_row!(
                store.route_members(),
                rows::route_member(row),
                present.route_members.contains(&row.id)
            );
        }
        for row in &data.exposed_models {
            known.require(
                &known.routes,
                Some(row.route_id.as_str()),
                "exposed model",
                &row.id,
                "route",
            )?;
            write_row!(
                store.exposed_models(),
                rows::exposed_model(row),
                present.exposed_models.contains(&row.id)
            );
        }
        for row in &data.operation_rules {
            known.require(
                &known.providers,
                Some(row.provider_id.as_str()),
                "operation rule",
                &row.id,
                "provider",
            )?;
            write_row!(
                store.operation_rules(),
                rows::operation_rule(row),
                present.operation_rules.contains(&row.id)
            );
        }
        for row in &data.operation_endpoints {
            known.require(
                &known.providers,
                Some(row.provider_id.as_str()),
                "operation endpoint",
                &row.id,
                "provider",
            )?;
            write_row!(
                store.operation_endpoints(),
                rows::operation_endpoint(row)?,
                present.operation_endpoints.contains(&row.id)
            );
        }
        for row in &data.rewrite_rule_sets {
            write_row!(
                store.rewrite_rule_sets(),
                rows::rule_set(row),
                present.rule_sets.contains(&row.id)
            );
        }
        for row in &data.rewrite_rules {
            known.require(
                &known.rule_sets,
                Some(row.rule_set_id.as_str()),
                "rewrite rule",
                &row.id,
                "rewrite rule set",
            )?;
            write_row!(
                store.rewrite_rules(),
                rows::rewrite_rule(row)?,
                present.rewrite_rules.contains(&row.id)
            );
        }
        for row in &data.provider_rewrite_rule_sets {
            known.require(
                &known.providers,
                Some(row.provider_id.as_str()),
                "provider rule set",
                &row.id,
                "provider",
            )?;
            known.require(
                &known.rule_sets,
                Some(row.rule_set_id.as_str()),
                "provider rule set",
                &row.id,
                "rewrite rule set",
            )?;
            write_row!(
                store.provider_rewrite_rule_sets(),
                rows::provider_rule_set(row),
                present.provider_rule_sets.contains(&row.id)
            );
        }
        for row in &data.quotas {
            write_row!(
                store.quotas(),
                rows::quota(row)?,
                present.quotas.contains(&row.id)
            );
        }
        for row in &data.price_rules {
            known.require(
                &known.providers,
                row.provider_id.as_deref(),
                "price rule",
                &row.id,
                "provider",
            )?;
            write_row!(
                store.price_rules(),
                rows::price_rule(row),
                present.price_rules.contains(&row.id)
            );
        }
        for row in &data.price_rates {
            known.require(
                &known.price_rules,
                Some(row.price_rule_id.as_str()),
                "price rate",
                &row.id,
                "price rule",
            )?;
            write_row!(
                store.price_rates(),
                rows::price_rate(row)?,
                present.price_rates.contains(&row.id)
            );
        }
        for row in &data.price_tiers {
            known.require(
                &known.price_rules,
                Some(row.price_rule_id.as_str()),
                "price tier",
                &row.id,
                "price rule",
            )?;
            write_row!(
                store.price_tiers(),
                rows::price_tier(row)?,
                present.price_tiers.contains(&row.id)
            );
        }

        if let Some(settings) = &data.settings {
            statements.push(BatchStatement::Execute(store.settings().update_statement(
                rows::settings(settings, &known, &files, &mut report.warnings),
            )?));
            report.updated += 1;
        }

        self.writer
            .commit(
                statements,
                &[
                    Scope::Profiles,
                    Scope::Providers,
                    Scope::Credentials(Vec::new()),
                    Scope::Models,
                    Scope::Routing,
                    Scope::Endpoints,
                    Scope::Rewrite,
                    Scope::Quotas,
                    Scope::Pricing,
                    Scope::Settings,
                ],
            )
            .await?;
        Ok(report)
    }

    /// The sealed bytes to store, or None when this credential must be left
    /// out. See the table at the top of the module.
    fn secret(
        &self,
        row: &ExportCredentialDto,
        exists: bool,
        source: Option<&AesGcmCodec>,
        report: &mut ImportReportDto,
    ) -> SdkResult<Option<Option<Vec<u8>>>> {
        let id = &row.credential.id;
        let Some(sealed) = &row.secret else {
            // A configuration-only export cannot create a credential: core
            // opens every secret while assembling, so a row without one would
            // fail every reload rather than just this credential's calls.
            if exists {
                report.warnings.push(format!(
                    "credential `{id}` kept the secret it already had: the export carried none"
                ));
                return Ok(Some(None));
            }
            report.credentials_skipped += 1;
            report.skipped += 1;
            report.warnings.push(format!(
                "credential `{id}` was not imported: the export carried no secret for it"
            ));
            return Ok(None);
        };
        let bytes = B64.decode(sealed.bytes.as_bytes()).map_err(|error| {
            SdkError::invalid(format!(
                "credential `{id}` has an unreadable sealed secret: {error}"
            ))
        })?;
        match source {
            // With the source key the blob is opened once and resealed under
            // this instance's codec, which is what makes two deployments with
            // different keys mergeable at all.
            Some(codec) => match codec.open(id, &bytes) {
                Ok(secret) => {
                    let resealed = self.writer.core().secret_codec().seal(id, &secret)?;
                    report.credentials_resealed += 1;
                    Ok(Some(Some(resealed)))
                }
                Err(_) => {
                    report.credentials_skipped += 1;
                    report.skipped += 1;
                    report.warnings.push(format!(
                        "credential `{id}` was not imported: its secret did not open with the supplied source master key"
                    ));
                    Ok(None)
                }
            },
            // Without one the blob is taken as it is, which only works when
            // both instances seal the same way. Storing one this instance
            // cannot open would break its next reload, so that is refused
            // rather than written.
            None => match self.writer.core().secret_codec().open(id, &bytes) {
                Ok(_) => {
                    report.warnings.push(format!(
                        "credential `{id}` was imported with its sealed bytes unchanged: no source master key was supplied"
                    ));
                    Ok(Some(Some(bytes)))
                }
                Err(_) => {
                    report.credentials_skipped += 1;
                    report.skipped += 1;
                    report.warnings.push(format!(
                        "credential `{id}` was not imported: its secret is sealed for another key; supply sourceMasterKey to reseal it"
                    ));
                    Ok(None)
                }
            },
        }
    }

    /// Delete every row of an exported kind the document does not mention,
    /// children before parents. Nothing outside those kinds is touched: an
    /// import must never take a user, an API key or a usage record with it.
    fn deletions(
        &self,
        data: &ConfigurationDataDto,
        current: &gproxy_store::ControlData,
        routing: &gproxy_store::RoutingData,
        statements: &mut Vec<BatchStatement>,
        report: &mut ImportReportDto,
    ) {
        let store = self.writer.store();
        let mut removed = 0u64;
        macro_rules! prune {
            ($repository:expr, $current:expr, $keep:expr) => {{
                let keep: HashSet<&str> = $keep;
                for row in $current {
                    if !keep.contains(row.id.as_str()) {
                        statements.push(BatchStatement::Execute(
                            $repository.delete_statement(row.id.clone()),
                        ));
                        removed += 1;
                    }
                }
            }};
        }
        prune!(
            store.price_tiers(),
            &current.price_tiers,
            id_set(&data.price_tiers, |row| &row.id)
        );
        prune!(
            store.price_rates(),
            &current.price_rates,
            id_set(&data.price_rates, |row| &row.id)
        );
        prune!(
            store.price_rules(),
            &current.price_rules,
            id_set(&data.price_rules, |row| &row.id)
        );
        prune!(
            store.quotas(),
            &current.quotas,
            id_set(&data.quotas, |row| &row.id)
        );
        prune!(
            store.provider_rewrite_rule_sets(),
            &current.provider_rewrite_rule_sets,
            id_set(&data.provider_rewrite_rule_sets, |row| &row.id)
        );
        prune!(
            store.rewrite_rules(),
            &current.rewrite_rules,
            id_set(&data.rewrite_rules, |row| &row.id)
        );
        prune!(
            store.rewrite_rule_sets(),
            &current.rewrite_rule_sets,
            id_set(&data.rewrite_rule_sets, |row| &row.id)
        );
        prune!(
            store.operation_endpoints(),
            &current.operation_endpoints,
            id_set(&data.operation_endpoints, |row| &row.id)
        );
        prune!(
            store.operation_rules(),
            &current.operation_rules,
            id_set(&data.operation_rules, |row| &row.id)
        );
        prune!(
            store.exposed_models(),
            &routing.exposed_models,
            id_set(&data.exposed_models, |row| &row.id)
        );
        prune!(
            store.route_members(),
            &routing.route_members,
            id_set(&data.route_members, |row| &row.id)
        );
        prune!(
            store.routes(),
            &routing.routes,
            id_set(&data.routes, |row| &row.id)
        );
        prune!(
            store.provider_models(),
            &current.provider_models,
            id_set(&data.provider_models, |row| &row.id)
        );
        prune!(
            store.models(),
            &current.models,
            id_set(&data.models, |row| &row.id)
        );
        prune!(
            store.credentials(),
            &current.credentials,
            data.credentials
                .iter()
                .map(|row| row.credential.id.as_str())
                .collect()
        );
        prune!(
            store.providers(),
            &current.providers,
            id_set(&data.providers, |row| &row.id)
        );
        prune!(
            store.connection_profiles(),
            &current.connection_profiles,
            id_set(&data.connection_profiles, |row| &row.id)
        );
        if removed > 0 {
            report.warnings.push(format!(
                "replace removed {removed} rows the document did not mention"
            ));
        }
    }

    /// The file objects the document points at that actually exist here. File
    /// objects are storage rather than configuration and never travel, so a
    /// vocabulary reference that misses is cleared rather than refused.
    async fn existing_files(&self, data: &ConfigurationDataDto) -> SdkResult<HashSet<String>> {
        let wanted: Vec<String> = data
            .models
            .iter()
            .filter_map(|row| row.vocabulary_file_id.clone())
            .chain(
                data.settings
                    .as_ref()
                    .and_then(|settings| settings.instance.default_vocabulary_file_id.clone()),
            )
            .collect();
        self.present(self.writer.store().file_objects(), wanted)
            .await
    }

    /// The credential owners the document points at that exist here. The three
    /// owner columns belong to the application layer's identity tables, which
    /// this crate neither exports nor imports.
    async fn existing_owners(&self, data: &ConfigurationDataDto) -> SdkResult<Owners> {
        let collect = |pick: fn(&CredentialDto) -> Option<&String>| -> Vec<String> {
            data.credentials
                .iter()
                .filter_map(|row| pick(&row.credential).cloned())
                .collect()
        };
        Ok(Owners {
            organizations: self
                .present(
                    self.writer.store().organizations(),
                    collect(|row| row.organization_id.as_ref()),
                )
                .await?,
            teams: self
                .present(
                    self.writer.store().teams(),
                    collect(|row| row.team_id.as_ref()),
                )
                .await?,
            users: self
                .present(
                    self.writer.store().users(),
                    collect(|row| row.user_id.as_ref()),
                )
                .await?,
        })
    }

    async fn present<E>(
        &self,
        repository: gproxy_store::Repository<'_, C, E>,
        mut ids: Vec<String>,
    ) -> SdkResult<HashSet<String>>
    where
        E: sea_orm::EntityTrait<PrimaryKey: sea_orm::PrimaryKeyTrait<ValueType = String>>,
    {
        ids.sort();
        ids.dedup();
        if ids.is_empty() {
            return Ok(HashSet::new());
        }
        Ok(repository
            .get_many(&ids)
            .await?
            .into_iter()
            .zip(ids)
            .filter_map(|(row, id)| row.map(|_| id))
            .collect())
    }
}

/// The identity rows a credential may name, restricted to the ones this
/// instance has.
struct Owners {
    organizations: HashSet<String>,
    teams: HashSet<String>,
    users: HashSet<String>,
}

/// Which ids a document reference may resolve against.
struct Known {
    profiles: HashSet<String>,
    providers: HashSet<String>,
    models: HashSet<String>,
    routes: HashSet<String>,
    rule_sets: HashSet<String>,
    price_rules: HashSet<String>,
}

impl Known {
    /// A reference that resolves nowhere is refused before anything is
    /// written: a foreign-key error in the middle of the batch would say far
    /// less about which row of the document is wrong.
    fn require(
        &self,
        set: &HashSet<String>,
        value: Option<&str>,
        entity: &'static str,
        id: &str,
        target: &'static str,
    ) -> SdkResult<()> {
        match value {
            Some(value) if !set.contains(value) => Err(SdkError::invalid(format!(
                "{entity} `{id}` names {target} `{value}`, which is neither in the export nor here"
            ))),
            _ => Ok(()),
        }
    }
}

/// Every id present in the destination, one set per exported table.
struct Present {
    profiles: HashSet<String>,
    providers: HashSet<String>,
    credentials: HashSet<String>,
    models: HashSet<String>,
    provider_models: HashSet<String>,
    routes: HashSet<String>,
    route_members: HashSet<String>,
    exposed_models: HashSet<String>,
    operation_rules: HashSet<String>,
    operation_endpoints: HashSet<String>,
    rule_sets: HashSet<String>,
    rewrite_rules: HashSet<String>,
    provider_rule_sets: HashSet<String>,
    quotas: HashSet<String>,
    price_rules: HashSet<String>,
    price_rates: HashSet<String>,
    price_tiers: HashSet<String>,
}

fn map<M, D: From<M>>(rows: Vec<M>) -> Vec<D> {
    rows.into_iter().map(D::from).collect()
}

fn ids<T>(rows: &[T], pick: fn(&T) -> &String) -> HashSet<String> {
    rows.iter().map(|row| pick(row).clone()).collect()
}

fn id_set<T>(rows: &[T], pick: fn(&T) -> &String) -> HashSet<&str> {
    rows.iter().map(|row| pick(row).as_str()).collect()
}

fn reach<T>(
    mut document: HashSet<String>,
    current: &[T],
    pick: fn(&T) -> &String,
    merge: bool,
) -> HashSet<String> {
    if merge {
        document.extend(current.iter().map(|row| pick(row).clone()));
    }
    document
}

/// The codec an envelope announces about itself.
fn codec_name(sealed: &[u8]) -> &'static str {
    match sealed.first() {
        Some(&ENVELOPE_PLAINTEXT) => CODEC_PLAINTEXT,
        Some(&ENVELOPE_AES_GCM) => CODEC_AES_GCM,
        _ => CODEC_UNKNOWN,
    }
}

fn master_key(value: &str) -> SdkResult<AesGcmCodec> {
    let bytes = B64
        .decode(value.trim().as_bytes())
        .map_err(|_| SdkError::invalid("sourceMasterKey must be standard base64"))?;
    let key: [u8; 32] = bytes
        .try_into()
        .map_err(|_| SdkError::invalid("sourceMasterKey must decode to 32 bytes"))?;
    Ok(AesGcmCodec::new(key))
}

/// DTO to row, column for column. Nothing here defaults or invents a value:
/// an import writes what the document says, or refuses it.
mod rows {
    use super::*;

    pub(super) fn profile(dto: &ConnectionProfileDto) -> SdkResult<profile::ActiveModel> {
        Ok(profile::ActiveModel {
            id: Set(dto.id.clone()),
            name: Set(dto.name.clone()),
            backend: Set(crud::enumerated(&dto.backend, "backend", &BACKENDS)?),
            proxy_mode: Set(crud::enumerated(
                &dto.proxy_mode,
                "proxyMode",
                &PROXY_MODES,
            )?),
            proxy_url: Set(dto.proxy_url.clone()),
            emulation: Set(dto.emulation.clone()),
            gzip: Set(dto.gzip),
            brotli: Set(dto.brotli),
            deflate: Set(dto.deflate),
            zstd: Set(dto.zstd),
            redirect_max_hops: Set(dto.redirect_max_hops),
            retry: Set(crud::enumerated(&dto.retry, "retry", &RETRIES)?),
            connect_timeout_ms: Set(dto.connect_timeout_ms),
            pool_idle_timeout_ms: Set(dto.pool_idle_timeout_ms),
            pool_max_idle_per_host: Set(dto.pool_max_idle_per_host),
            created_at_ms: Set(dto.created_at_ms),
        })
    }

    pub(super) fn provider(dto: &ProviderDto) -> SdkResult<provider::ActiveModel> {
        Ok(provider::ActiveModel {
            id: Set(dto.id.clone()),
            name: Set(crud::text(&dto.name, "name")?),
            channel: Set(crud::text(&dto.channel, "channel")?),
            base_url: Set(dto.base_url.clone()),
            connection_profile_id: Set(dto.connection_profile_id.clone()),
            config: Set(crud::object(Some(dto.config.clone()), "config")?),
            enabled: Set(dto.enabled),
            created_at_ms: Set(dto.created_at_ms),
        })
    }

    /// `secret` is `None` when an update must keep the bytes already stored;
    /// leaving the column unset is what does that.
    pub(super) fn credential(
        dto: &ExportCredentialDto,
        secret: Option<Vec<u8>>,
        owners: &Owners,
        warnings: &mut Vec<String>,
    ) -> SdkResult<credential::ActiveModel> {
        let row = &dto.credential;
        let mut owner = |value: &Option<String>, set: &HashSet<String>, kind: &str| match value {
            Some(id) if !set.contains(id) => {
                warnings.push(format!(
                    "credential `{}` lost its {kind} `{id}`: no such row on this instance",
                    row.id
                ));
                None
            }
            other => other.clone(),
        };
        let mut model = credential::ActiveModel {
            id: Set(row.id.clone()),
            provider_id: Set(row.provider_id.clone()),
            organization_id: Set(owner(
                &row.organization_id,
                &owners.organizations,
                "organization",
            )),
            team_id: Set(owner(&row.team_id, &owners.teams, "team")),
            user_id: Set(owner(&row.user_id, &owners.users, "user")),
            label: Set(row.label.clone()),
            auth_kind: Set(crud::text(&row.auth_kind, "authKind")?),
            version: Set(row.version),
            connection_profile_id: Set(row.connection_profile_id.clone()),
            metadata: Set(crud::object(Some(row.metadata.clone()), "metadata")?),
            expires_at_ms: Set(row.expires_at_ms),
            status: Set(crud::enumerated(&row.status, "status", &STATUSES)?),
            status_reason: Set(row.status_reason.clone()),
            enabled: Set(row.enabled),
            ..Default::default()
        };
        if let Some(secret) = secret {
            model.secret = Set(secret);
        }
        Ok(model)
    }

    pub(super) fn model(
        dto: &ModelDto,
        files: &HashSet<String>,
        warnings: &mut Vec<String>,
    ) -> model::ActiveModel {
        let vocabulary = match &dto.vocabulary_file_id {
            Some(id) if !files.contains(id) => {
                warnings.push(format!(
                    "model `{}` lost its vocabulary file `{id}`: file objects do not travel with a configuration export",
                    dto.id
                ));
                None
            }
            other => other.clone(),
        };
        model::ActiveModel {
            id: Set(dto.id.clone()),
            name: Set(dto.name.clone()),
            metadata: Set(dto.metadata.clone()),
            vocabulary_file_id: Set(vocabulary),
        }
    }

    pub(super) fn provider_model(dto: &ProviderModelDto) -> provider_model::ActiveModel {
        provider_model::ActiveModel {
            id: Set(dto.id.clone()),
            provider_id: Set(dto.provider_id.clone()),
            upstream_name: Set(dto.upstream_name.clone()),
            model_id: Set(dto.model_id.clone()),
            metadata: Set(dto.metadata.clone()),
            enabled: Set(dto.enabled),
        }
    }

    pub(super) fn route(dto: &RouteDto) -> SdkResult<route::ActiveModel> {
        Ok(route::ActiveModel {
            id: Set(dto.id.clone()),
            name: Set(dto.name.clone()),
            strategy: Set(crud::enumerated(&dto.strategy, "strategy", &STRATEGIES)?),
            max_attempts: Set(dto.max_attempts.max(1)),
            enabled: Set(dto.enabled),
        })
    }

    pub(super) fn route_member(dto: &RouteMemberDto) -> route_member::ActiveModel {
        route_member::ActiveModel {
            id: Set(dto.id.clone()),
            route_id: Set(dto.route_id.clone()),
            provider_id: Set(dto.provider_id.clone()),
            upstream_model: Set(dto.upstream_model.clone()),
            tier: Set(dto.tier),
            weight: Set(dto.weight.max(1)),
            enabled: Set(dto.enabled),
        }
    }

    pub(super) fn exposed_model(dto: &ExposedModelDto) -> exposed_model::ActiveModel {
        exposed_model::ActiveModel {
            id: Set(dto.id.clone()),
            name: Set(dto.name.clone()),
            route_id: Set(dto.route_id.clone()),
            enabled: Set(dto.enabled),
        }
    }

    pub(super) fn operation_rule(dto: &OperationRuleDto) -> operation_rule::ActiveModel {
        operation_rule::ActiveModel {
            id: Set(dto.id.clone()),
            provider_id: Set(dto.provider_id.clone()),
            operation: Set(dto.operation.clone()),
            action: Set(dto.action.clone()),
            target: Set(dto.target.clone()),
        }
    }

    pub(super) fn operation_endpoint(
        dto: &OperationEndpointDto,
    ) -> SdkResult<operation_endpoint::ActiveModel> {
        Ok(operation_endpoint::ActiveModel {
            id: Set(dto.id.clone()),
            provider_id: Set(dto.provider_id.clone()),
            operation: Set(dto.operation.clone()),
            dialect: Set(dto.dialect.clone()),
            transport: Set(crud::enumerated(&dto.transport, "transport", &TRANSPORTS)?),
            url: Set(crud::url(&dto.url, "url")?),
            enabled: Set(dto.enabled),
        })
    }

    pub(super) fn rule_set(dto: &RuleSetDto) -> rewrite_rule_set::ActiveModel {
        rewrite_rule_set::ActiveModel {
            id: Set(dto.id.clone()),
            name: Set(dto.name.clone()),
            description: Set(dto.description.clone()),
            enabled: Set(dto.enabled),
            created_at_ms: Set(dto.created_at_ms),
            updated_at_ms: Set(dto.updated_at_ms),
        }
    }

    pub(super) fn rewrite_rule(dto: &RewriteRuleDto) -> SdkResult<rewrite_rule::ActiveModel> {
        let row = rewrite_rule::Model {
            id: dto.id.clone(),
            rule_set_id: dto.rule_set_id.clone(),
            phase: dto.phase.clone(),
            target: crud::enumerated(&dto.target, "target", &TARGETS)?,
            target_name: dto.target_name.clone(),
            paths: dto.paths.clone(),
            pattern: dto.pattern.clone(),
            replacement: dto.replacement.clone(),
            filter_operation_keys: dto.filter_operation_keys.clone(),
            filter_model_pattern: dto.filter_model_pattern.clone(),
            filter_header_pattern: dto.filter_header_pattern.clone(),
            filter_event_pattern: dto.filter_event_pattern.clone(),
            sort_order: dto.sort_order,
            enabled: dto.enabled,
            created_at_ms: dto.created_at_ms,
            updated_at_ms: dto.updated_at_ms,
        };
        // The same compiler the ordinary rewrite family runs: an imported rule
        // that core would refuse to load is refused here instead.
        gproxy_core::rewrite::compile_rule(std::sync::Arc::new(row.clone())).map_err(|error| {
            SdkError::invalid(format!("rewrite rule `{}` is not usable: {error}", row.id))
        })?;
        Ok(rewrite_rule::ActiveModel {
            id: Set(row.id),
            rule_set_id: Set(row.rule_set_id),
            phase: Set(row.phase),
            target: Set(row.target),
            target_name: Set(row.target_name),
            paths: Set(row.paths),
            pattern: Set(row.pattern),
            replacement: Set(row.replacement),
            filter_operation_keys: Set(row.filter_operation_keys),
            filter_model_pattern: Set(row.filter_model_pattern),
            filter_header_pattern: Set(row.filter_header_pattern),
            filter_event_pattern: Set(row.filter_event_pattern),
            sort_order: Set(row.sort_order),
            enabled: Set(row.enabled),
            created_at_ms: Set(row.created_at_ms),
            updated_at_ms: Set(row.updated_at_ms),
        })
    }

    pub(super) fn provider_rule_set(
        dto: &ProviderRuleSetDto,
    ) -> provider_rewrite_rule_set::ActiveModel {
        provider_rewrite_rule_set::ActiveModel {
            id: Set(dto.id.clone()),
            provider_id: Set(dto.provider_id.clone()),
            rule_set_id: Set(dto.rule_set_id.clone()),
            sort_order: Set(dto.sort_order),
            enabled: Set(dto.enabled),
            created_at_ms: Set(dto.created_at_ms),
            updated_at_ms: Set(dto.updated_at_ms),
        }
    }

    pub(super) fn quota(dto: &QuotaDto) -> SdkResult<quota::ActiveModel> {
        Ok(quota::ActiveModel {
            id: Set(dto.id.clone()),
            owner_kind: Set(crud::text(&dto.owner_kind, "ownerKind")?),
            owner_id: Set(crud::text(&dto.owner_id, "ownerId")?),
            window_key: Set(dto.window_key.clone()),
            metric: Set(dto.metric.clone()),
            unit: Set(dto.unit.clone()),
            limit_value: Set(crud::decimal(&dto.limit_value, "limitValue")?),
            period: Set(dto.period.clone()),
            period_seconds: Set(dto.period_seconds),
            anchor_at_ms: Set(dto.anchor_at_ms),
            model_pattern: Set(dto.model_pattern.clone()),
            enabled: Set(dto.enabled),
        })
    }

    pub(super) fn price_rule(dto: &PriceRuleDto) -> price_rule::ActiveModel {
        price_rule::ActiveModel {
            id: Set(dto.id.clone()),
            provider_id: Set(dto.provider_id.clone()),
            model_pattern: Set(dto.model_pattern.clone()),
            operation: Set(dto.operation.clone()),
            priority: Set(dto.priority),
            currency: Set(dto.currency.clone()),
            enabled: Set(dto.enabled),
        }
    }

    pub(super) fn price_rate(dto: &PriceRateDto) -> SdkResult<price_rate::ActiveModel> {
        Ok(price_rate::ActiveModel {
            id: Set(dto.id.clone()),
            price_rule_id: Set(dto.price_rule_id.clone()),
            metric: Set(dto.metric.clone()),
            unit: Set(crud::enumerated(&dto.unit, "unit", &UNITS)?),
            unit_quantity: Set(crud::decimal(&dto.unit_quantity, "unitQuantity")?),
            value: Set(crud::decimal(&dto.value, "value")?),
            conditions: Set(dto.conditions.clone()),
            priority: Set(dto.priority),
        })
    }

    pub(super) fn price_tier(dto: &PriceTierDto) -> SdkResult<price_tier::ActiveModel> {
        let money = |value: &Option<String>, field: &'static str| match value {
            Some(value) => crud::decimal(value, field).map(Some),
            None => Ok(None),
        };
        Ok(price_tier::ActiveModel {
            id: Set(dto.id.clone()),
            price_rule_id: Set(dto.price_rule_id.clone()),
            service_tier: Set(dto.service_tier.clone()),
            min_prompt_tokens: Set(dto.min_prompt_tokens),
            priority: Set(dto.priority),
            multiplier: Set(money(&dto.multiplier, "multiplier")?),
            input_per_million: Set(money(&dto.input_per_million, "inputPerMillion")?),
            output_per_million: Set(money(&dto.output_per_million, "outputPerMillion")?),
            cache_read_per_million: Set(money(&dto.cache_read_per_million, "cacheReadPerMillion")?),
            cache_creation_5m_per_million: Set(money(
                &dto.cache_creation_5m_per_million,
                "cacheCreation5mPerMillion",
            )?),
            cache_creation_30m_per_million: Set(money(
                &dto.cache_creation_30m_per_million,
                "cacheCreation30mPerMillion",
            )?),
            cache_creation_1h_per_million: Set(money(
                &dto.cache_creation_1h_per_million,
                "cacheCreation1hPerMillion",
            )?),
            reasoning_per_million: Set(money(&dto.reasoning_per_million, "reasoningPerMillion")?),
            image_input_per_million: Set(money(
                &dto.image_input_per_million,
                "imageInputPerMillion",
            )?),
            image_output_per_million: Set(money(
                &dto.image_output_per_million,
                "imageOutputPerMillion",
            )?),
            audio_input_per_million: Set(money(
                &dto.audio_input_per_million,
                "audioInputPerMillion",
            )?),
            cached_audio_input_per_million: Set(money(
                &dto.cached_audio_input_per_million,
                "cachedAudioInputPerMillion",
            )?),
            audio_output_per_million: Set(money(
                &dto.audio_output_per_million,
                "audioOutputPerMillion",
            )?),
            video_input_per_million: Set(money(
                &dto.video_input_per_million,
                "videoInputPerMillion",
            )?),
            video_per_million: Set(money(&dto.video_per_million, "videoPerMillion")?),
        })
    }

    /// Everything the settings row holds except `config_revision`, which is
    /// this instance's own counter, and the sealed tokenizer token, which no
    /// DTO carries.
    pub(super) fn settings(
        dto: &SettingsDto,
        known: &Known,
        files: &HashSet<String>,
        warnings: &mut Vec<String>,
    ) -> setting::ActiveModel {
        let instance = &dto.instance;
        let logging = &dto.logging;
        let profile = match &instance.connection_profile_id {
            Some(id) if !known.profiles.contains(id) => {
                warnings.push(format!(
                    "settings lost their default connection profile `{id}`: it is neither in the export nor here"
                ));
                None
            }
            other => other.clone(),
        };
        let vocabulary = match &instance.default_vocabulary_file_id {
            Some(id) if !files.contains(id) => {
                warnings.push(format!(
                    "settings lost their default vocabulary file `{id}`: file objects do not travel with a configuration export"
                ));
                None
            }
            other => other.clone(),
        };
        setting::ActiveModel {
            id: Set(setting::GLOBAL_SETTINGS_ID),
            instance_name: Set(instance.instance_name.clone()),
            oauth_client_allowlist: Set(instance.oauth_client_allowlist.clone()),
            connection_profile_id: Set(profile),
            cors_origins: Set(instance.cors_origins.clone()),
            trusted_proxies: Set(instance.trusted_proxies.clone()),
            max_attempts: Set(instance.max_attempts.max(1)),
            max_in_flight: Set(instance.max_in_flight),
            file_upload_max_in_flight: Set(instance.file_upload_max_in_flight),
            enable_settlement: Set(instance.enable_settlement),
            enable_usage: Set(instance.enable_usage),
            request_timeout_ms: Set(instance.request_timeout_ms),
            stream_idle_timeout_ms: Set(instance.stream_idle_timeout_ms),
            max_request_body_bytes: Set(instance.max_request_body_bytes),
            max_response_body_bytes: Set(instance.max_response_body_bytes),
            max_stream_event_bytes: Set(instance.max_stream_event_bytes),
            max_ws_frame_bytes: Set(instance.max_ws_frame_bytes),
            max_multipart_parts: Set(instance.max_multipart_parts),
            enable_tokenizer_vocabs: Set(instance.enable_tokenizer_vocabs),
            enable_tokenizer_download: Set(instance.enable_tokenizer_download),
            default_vocabulary_file_id: Set(vocabulary),
            default_file_storage_name: Set(instance.default_file_storage_name.clone()),
            retention_days: Set(instance.retention_days),
            max_database_size_mb: Set(instance.max_database_size_mb),
            update_channel: Set(instance.update_channel.clone()),
            enable_auto_update_check: Set(instance.enable_auto_update_check),
            portal_recent_requests_enabled: Set(instance.portal_recent_requests_enabled),
            enable_downstream_log: Set(logging.enable_downstream_log),
            enable_downstream_log_body: Set(logging.enable_downstream_log_body),
            enable_upstream_log: Set(logging.enable_upstream_log),
            enable_upstream_log_body: Set(logging.enable_upstream_log_body),
            disable_log_redaction: Set(logging.disable_log_redaction),
            enable_tracing: Set(logging.enable_tracing),
            log_level: Set(logging.log_level.clone()),
            log_format: Set(logging.log_format.clone()),
            request_header_blacklist: Set(logging.request_header_blacklist.clone()),
            response_header_blacklist: Set(logging.response_header_blacklist.clone()),
            query_parameter_blacklist: Set(logging.query_parameter_blacklist.clone()),
            ..Default::default()
        }
    }
}
