//! Pricing: which rule covers a model, what each quantity costs, and the
//! context or service-tier overrides inside a rule.

use gproxy_seaorm::BatchConnectionTrait;
use gproxy_store::{
    FixedDecimal, Repository,
    entity::pricing::{price_rate, price_rule, price_tier, price_unit::PriceUnit},
};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, Select, Set};

use super::{
    Scope, Writer,
    crud::{self, Shape},
};
use crate::{
    SdkError, SdkResult,
    dto::{
        BatchItem, ListQuery, Page, PriceRateDto, PriceRatePatch, PriceRateWrite, PriceRuleDto,
        PriceRulePatch, PriceRuleWrite, PriceTierDto, PriceTierPatch, PriceTierWrite,
    },
};

const UNITS: [&str; 4] = ["token", "count", "second", "character"];

pub struct Pricing<'a, C> {
    writer: Writer<'a, C>,
}

impl<'a, C> Pricing<'a, C> {
    pub(crate) fn new(writer: Writer<'a, C>) -> Self {
        Self { writer }
    }
    pub fn rules(&self) -> PriceRules<'a, C> {
        PriceRules {
            writer: self.writer,
        }
    }
    pub fn rates(&self) -> PriceRates<'a, C> {
        PriceRates {
            writer: self.writer,
        }
    }
    pub fn tiers(&self) -> PriceTiers<'a, C> {
        PriceTiers {
            writer: self.writer,
        }
    }
}

/// All prices and settled amounts use USD. Normalize accepted spelling once.
pub(super) fn currency(value: &str) -> SdkResult<String> {
    let value = crud::text(value, "currency")?.to_ascii_uppercase();
    if value != "USD" {
        return Err(SdkError::invalid("currency must be USD"));
    }
    Ok(value)
}

/// Optional decimal columns of a tier: absent leaves the column alone, `null`
/// clears it, a string sets it.
fn optional_decimal(value: Option<String>, field: &'static str) -> SdkResult<Option<FixedDecimal>> {
    match crud::optional_text(value) {
        Some(value) => Ok(Some(crud::decimal(&value, field)?)),
        None => Ok(None),
    }
}

pub struct PriceRules<'a, C> {
    writer: Writer<'a, C>,
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> PriceRules<'_, C> {
    pub async fn list(&self, query: ListQuery) -> SdkResult<Page<PriceRuleDto>> {
        crud::list(self, query).await
    }
    pub async fn get(&self, id: &str) -> SdkResult<PriceRuleDto> {
        crud::get(self, id).await
    }
    pub async fn create(&self, write: PriceRuleWrite) -> SdkResult<PriceRuleDto> {
        crud::create(self, write).await
    }
    pub async fn update(&self, id: &str, patch: PriceRulePatch) -> SdkResult<PriceRuleDto> {
        crud::update(self, id, patch).await
    }
    pub async fn delete(&self, id: &str) -> SdkResult<()> {
        crud::delete(self, id).await
    }
    pub async fn batch(
        &self,
        items: Vec<BatchItem<PriceRuleWrite, PriceRulePatch>>,
    ) -> SdkResult<Vec<Option<PriceRuleDto>>> {
        crud::batch(self, items).await
    }

    async fn provider(&self, id: Option<String>) -> SdkResult<Option<String>> {
        let Some(id) = crud::optional_text(id) else {
            return Ok(None);
        };
        crud::require_rows(
            self.writer.store().providers(),
            "provider",
            std::slice::from_ref(&id),
        )
        .await?;
        Ok(Some(id))
    }
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> Shape<C> for PriceRules<'_, C> {
    type Entity = price_rule::Entity;
    type Dto = PriceRuleDto;
    type Write = PriceRuleWrite;
    type Patch = PriceRulePatch;

    const ENTITY: &'static str = "price rule";

    fn writer(&self) -> Writer<'_, C> {
        self.writer
    }
    fn repository(&self) -> Repository<'_, C, Self::Entity> {
        self.writer.store().price_rules()
    }
    fn scopes(&self) -> Vec<Scope> {
        vec![Scope::Pricing]
    }
    fn select(&self, query: &ListQuery) -> Select<Self::Entity> {
        let mut select = price_rule::Entity::find();
        if query.global_only == Some(true) {
            select = select.filter(price_rule::Column::ProviderId.is_null());
        }
        if let Some(pattern) = &query.model_pattern {
            select = select.filter(price_rule::Column::ModelPattern.eq(pattern));
        }
        if let Some(provider_id) = crud::optional_text(query.provider_id.clone()) {
            select = select.filter(price_rule::Column::ProviderId.eq(provider_id));
        }
        if let Some(search) = crud::optional_text(query.search.clone()) {
            select = select.filter(price_rule::Column::ModelPattern.contains(&search));
        }
        if let Some(enabled) = query.enabled {
            select = select.filter(price_rule::Column::Enabled.eq(enabled));
        }
        select
    }

    async fn build(&self, write: PriceRuleWrite) -> SdkResult<(price_rule::ActiveModel, String)> {
        let id = crud::id_or_new(write.id.as_deref());
        let row = price_rule::ActiveModel {
            id: Set(id.clone()),
            provider_id: Set(self.provider(write.provider_id).await?),
            model_pattern: Set(crud::text(&write.model_pattern, "modelPattern")?),
            operation: Set(crud::optional_text(write.operation)),
            priority: Set(write.priority.unwrap_or(0)),
            currency: Set(currency(&write.currency)?),
            enabled: Set(write.enabled.unwrap_or(true)),
        };
        Ok((row, id))
    }

    async fn change(
        &self,
        current: &price_rule::Model,
        patch: PriceRulePatch,
    ) -> SdkResult<price_rule::ActiveModel> {
        let mut row = price_rule::ActiveModel {
            id: Set(current.id.clone()),
            ..Default::default()
        };
        if let Some(provider_id) = patch.provider_id {
            row.provider_id = Set(self.provider(provider_id).await?);
        }
        if let Some(pattern) = patch.model_pattern {
            row.model_pattern = Set(crud::text(&pattern, "modelPattern")?);
        }
        if let Some(operation) = patch.operation {
            row.operation = Set(crud::optional_text(operation));
        }
        if let Some(priority) = patch.priority {
            row.priority = Set(priority);
        }
        if let Some(value) = patch.currency {
            row.currency = Set(currency(&value)?);
        }
        if let Some(enabled) = patch.enabled {
            row.enabled = Set(enabled);
        }
        Ok(row)
    }
}

pub struct PriceRates<'a, C> {
    writer: Writer<'a, C>,
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> PriceRates<'_, C> {
    pub async fn list(&self, query: ListQuery) -> SdkResult<Page<PriceRateDto>> {
        crud::list(self, query).await
    }
    pub async fn get(&self, id: &str) -> SdkResult<PriceRateDto> {
        crud::get(self, id).await
    }
    pub async fn create(&self, write: PriceRateWrite) -> SdkResult<PriceRateDto> {
        crud::create(self, write).await
    }
    pub async fn update(&self, id: &str, patch: PriceRatePatch) -> SdkResult<PriceRateDto> {
        crud::update(self, id, patch).await
    }
    pub async fn delete(&self, id: &str) -> SdkResult<()> {
        crud::delete(self, id).await
    }
    pub async fn batch(
        &self,
        items: Vec<BatchItem<PriceRateWrite, PriceRatePatch>>,
    ) -> SdkResult<Vec<Option<PriceRateDto>>> {
        crud::batch(self, items).await
    }

    async fn rule(&self, id: &str) -> SdkResult<String> {
        let id = crud::text(id, "priceRuleId")?;
        crud::require_rows(
            self.writer.store().price_rules(),
            "price rule",
            std::slice::from_ref(&id),
        )
        .await?;
        Ok(id)
    }
}

/// The denominator a price is quoted against. Zero would divide the charge by
/// nothing at settlement.
fn unit_quantity(value: &str) -> SdkResult<FixedDecimal> {
    let quantity = crud::decimal(value, "unitQuantity")?;
    if quantity.atoms() <= 0 {
        return Err(SdkError::invalid("unitQuantity must be positive"));
    }
    Ok(quantity)
}

fn rate_value(value: &str) -> SdkResult<FixedDecimal> {
    let amount = crud::decimal(value, "value")?;
    if amount.atoms() < 0 {
        return Err(SdkError::invalid("value must not be negative"));
    }
    Ok(amount)
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> Shape<C> for PriceRates<'_, C> {
    type Entity = price_rate::Entity;
    type Dto = PriceRateDto;
    type Write = PriceRateWrite;
    type Patch = PriceRatePatch;

    const ENTITY: &'static str = "price rate";

    fn writer(&self) -> Writer<'_, C> {
        self.writer
    }
    fn repository(&self) -> Repository<'_, C, Self::Entity> {
        self.writer.store().price_rates()
    }
    fn scopes(&self) -> Vec<Scope> {
        vec![Scope::Pricing]
    }
    fn select(&self, query: &ListQuery) -> Select<Self::Entity> {
        let mut select = price_rate::Entity::find();
        if let Some(rule_id) = crud::optional_text(query.price_rule_id.clone()) {
            select = select.filter(price_rate::Column::PriceRuleId.eq(rule_id));
        }
        if let Some(search) = crud::optional_text(query.search.clone()) {
            select = select.filter(price_rate::Column::Metric.contains(&search));
        }
        select
    }

    async fn build(&self, write: PriceRateWrite) -> SdkResult<(price_rate::ActiveModel, String)> {
        let id = crud::id_or_new(write.id.as_deref());
        let row = price_rate::ActiveModel {
            id: Set(id.clone()),
            price_rule_id: Set(self.rule(&write.price_rule_id).await?),
            metric: Set(crud::text(&write.metric, "metric")?),
            unit: Set(crud::enumerated::<PriceUnit>(&write.unit, "unit", &UNITS)?),
            unit_quantity: Set(unit_quantity(&write.unit_quantity)?),
            value: Set(rate_value(&write.value)?),
            conditions: Set(write.conditions),
            priority: Set(write.priority.unwrap_or(0)),
        };
        Ok((row, id))
    }

    async fn change(
        &self,
        current: &price_rate::Model,
        patch: PriceRatePatch,
    ) -> SdkResult<price_rate::ActiveModel> {
        let mut row = price_rate::ActiveModel {
            id: Set(current.id.clone()),
            ..Default::default()
        };
        if let Some(metric) = patch.metric {
            row.metric = Set(crud::text(&metric, "metric")?);
        }
        if let Some(unit) = patch.unit {
            row.unit = Set(crud::enumerated::<PriceUnit>(&unit, "unit", &UNITS)?);
        }
        if let Some(quantity) = patch.unit_quantity {
            row.unit_quantity = Set(unit_quantity(&quantity)?);
        }
        if let Some(value) = patch.value {
            row.value = Set(rate_value(&value)?);
        }
        if let Some(conditions) = patch.conditions {
            row.conditions = Set(conditions);
        }
        if let Some(priority) = patch.priority {
            row.priority = Set(priority);
        }
        Ok(row)
    }
}

pub struct PriceTiers<'a, C> {
    writer: Writer<'a, C>,
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> PriceTiers<'_, C> {
    pub async fn list(&self, query: ListQuery) -> SdkResult<Page<PriceTierDto>> {
        crud::list(self, query).await
    }
    pub async fn get(&self, id: &str) -> SdkResult<PriceTierDto> {
        crud::get(self, id).await
    }
    pub async fn create(&self, write: PriceTierWrite) -> SdkResult<PriceTierDto> {
        crud::create(self, write).await
    }
    pub async fn update(&self, id: &str, patch: PriceTierPatch) -> SdkResult<PriceTierDto> {
        crud::update(self, id, patch).await
    }
    pub async fn delete(&self, id: &str) -> SdkResult<()> {
        crud::delete(self, id).await
    }
    pub async fn batch(
        &self,
        items: Vec<BatchItem<PriceTierWrite, PriceTierPatch>>,
    ) -> SdkResult<Vec<Option<PriceTierDto>>> {
        crud::batch(self, items).await
    }

    async fn rule(&self, id: &str) -> SdkResult<String> {
        let id = crud::text(id, "priceRuleId")?;
        crud::require_rows(
            self.writer.store().price_rules(),
            "price rule",
            std::slice::from_ref(&id),
        )
        .await?;
        Ok(id)
    }
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> Shape<C> for PriceTiers<'_, C> {
    type Entity = price_tier::Entity;
    type Dto = PriceTierDto;
    type Write = PriceTierWrite;
    type Patch = PriceTierPatch;

    const ENTITY: &'static str = "price tier";

    fn writer(&self) -> Writer<'_, C> {
        self.writer
    }
    fn repository(&self) -> Repository<'_, C, Self::Entity> {
        self.writer.store().price_tiers()
    }
    fn scopes(&self) -> Vec<Scope> {
        vec![Scope::Pricing]
    }
    fn select(&self, query: &ListQuery) -> Select<Self::Entity> {
        let mut select = price_tier::Entity::find();
        if let Some(rule_id) = crud::optional_text(query.price_rule_id.clone()) {
            select = select.filter(price_tier::Column::PriceRuleId.eq(rule_id));
        }
        select
    }

    async fn build(&self, write: PriceTierWrite) -> SdkResult<(price_tier::ActiveModel, String)> {
        let id = crud::id_or_new(write.id.as_deref());
        let minimum = write.min_prompt_tokens.unwrap_or(0);
        if minimum < 0 {
            return Err(SdkError::invalid("minPromptTokens must not be negative"));
        }
        let row = price_tier::ActiveModel {
            id: Set(id.clone()),
            price_rule_id: Set(self.rule(&write.price_rule_id).await?),
            service_tier: Set(crud::optional_text(write.service_tier)),
            min_prompt_tokens: Set(minimum),
            priority: Set(write.priority.unwrap_or(0)),
            multiplier: Set(optional_decimal(write.multiplier, "multiplier")?),
            input_per_million: Set(optional_decimal(
                write.input_per_million,
                "inputPerMillion",
            )?),
            output_per_million: Set(optional_decimal(
                write.output_per_million,
                "outputPerMillion",
            )?),
            cache_read_per_million: Set(optional_decimal(
                write.cache_read_per_million,
                "cacheReadPerMillion",
            )?),
            cache_creation_5m_per_million: Set(optional_decimal(
                write.cache_creation_5m_per_million,
                "cacheCreation5mPerMillion",
            )?),
            cache_creation_30m_per_million: Set(optional_decimal(
                write.cache_creation_30m_per_million,
                "cacheCreation30mPerMillion",
            )?),
            cache_creation_1h_per_million: Set(optional_decimal(
                write.cache_creation_1h_per_million,
                "cacheCreation1hPerMillion",
            )?),
            reasoning_per_million: Set(optional_decimal(
                write.reasoning_per_million,
                "reasoningPerMillion",
            )?),
            image_input_per_million: Set(optional_decimal(
                write.image_input_per_million,
                "imageInputPerMillion",
            )?),
            image_output_per_million: Set(optional_decimal(
                write.image_output_per_million,
                "imageOutputPerMillion",
            )?),
            audio_input_per_million: Set(optional_decimal(
                write.audio_input_per_million,
                "audioInputPerMillion",
            )?),
            cached_audio_input_per_million: Set(optional_decimal(
                write.cached_audio_input_per_million,
                "cachedAudioInputPerMillion",
            )?),
            audio_output_per_million: Set(optional_decimal(
                write.audio_output_per_million,
                "audioOutputPerMillion",
            )?),
            video_input_per_million: Set(optional_decimal(
                write.video_input_per_million,
                "videoInputPerMillion",
            )?),
            video_per_million: Set(optional_decimal(
                write.video_per_million,
                "videoPerMillion",
            )?),
        };
        Ok((row, id))
    }

    async fn change(
        &self,
        current: &price_tier::Model,
        patch: PriceTierPatch,
    ) -> SdkResult<price_tier::ActiveModel> {
        let mut row = price_tier::ActiveModel {
            id: Set(current.id.clone()),
            ..Default::default()
        };
        if let Some(service_tier) = patch.service_tier {
            row.service_tier = Set(crud::optional_text(service_tier));
        }
        if let Some(minimum) = patch.min_prompt_tokens {
            if minimum < 0 {
                return Err(SdkError::invalid("minPromptTokens must not be negative"));
            }
            row.min_prompt_tokens = Set(minimum);
        }
        if let Some(priority) = patch.priority {
            row.priority = Set(priority);
        }
        // One arm per column, because each is a distinct nullable decimal and
        // a loop would need the ActiveModel's fields by name anyway.
        macro_rules! decimals {
            ($($field:ident => $label:literal),+ $(,)?) => {$(
                if let Some(value) = patch.$field {
                    row.$field = Set(optional_decimal(value, $label)?);
                }
            )+};
        }
        decimals! {
            multiplier => "multiplier",
            input_per_million => "inputPerMillion",
            output_per_million => "outputPerMillion",
            cache_read_per_million => "cacheReadPerMillion",
            cache_creation_5m_per_million => "cacheCreation5mPerMillion",
            cache_creation_30m_per_million => "cacheCreation30mPerMillion",
            cache_creation_1h_per_million => "cacheCreation1hPerMillion",
            reasoning_per_million => "reasoningPerMillion",
            image_input_per_million => "imageInputPerMillion",
            image_output_per_million => "imageOutputPerMillion",
            audio_input_per_million => "audioInputPerMillion",
            cached_audio_input_per_million => "cachedAudioInputPerMillion",
            audio_output_per_million => "audioOutputPerMillion",
            video_input_per_million => "videoInputPerMillion",
            video_per_million => "videoPerMillion",
        }
        Ok(row)
    }
}
