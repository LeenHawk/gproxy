use gproxy_channel_api::{NormalizedUsage, SessionUsage, SessionUsageKind};
use rust_decimal::Decimal;

use crate::control::{ControlPlane, ProviderRef};
use crate::error::CoreError;

use super::super::FunnelCtx;

pub(super) struct Totals {
    pub usage: NormalizedUsage,
    pub cost: Decimal,
}

impl Totals {
    pub(super) fn new() -> Self {
        Self {
            usage: NormalizedUsage::default(),
            cost: Decimal::ZERO,
        }
    }

    pub(super) fn add(
        &mut self,
        sample: SessionUsage,
        control: &dyn ControlPlane,
        provider: &ProviderRef,
        requested_tier: Option<&str>,
    ) -> Result<(), CoreError> {
        let mut price = control.pricing(provider, &sample.model);
        if sample.kind == SessionUsageKind::Primary
            && let (Some(price), Some(tier)) = (price.as_mut(), requested_tier)
        {
            price.service_tier = Some(tier.into());
        }
        let event_cost = price
            .as_ref()
            .map_or(Decimal::ZERO, |price| price.cost(&sample.usage));
        if price.is_none() {
            tracing::warn!(model = sample.model, "Realtime session pricing missing");
        }
        let kind = match sample.kind {
            SessionUsageKind::Primary => "primary",
            SessionUsageKind::Transcription => "transcription",
        };
        let prefix = format!("session_model/{kind}/{}", encode_segment(&sample.model));
        self.add_metric(
            &format!("{prefix}/input_tokens"),
            Decimal::from(sample.usage.input_tokens),
        )?;
        self.add_metric(
            &format!("{prefix}/output_tokens"),
            Decimal::from(sample.usage.output_tokens),
        )?;
        self.add_metric(
            &format!("{prefix}/cached_input_tokens"),
            Decimal::from(sample.usage.cached_input_tokens),
        )?;
        self.add_metric(&format!("{prefix}/cost"), event_cost)?;
        for (name, amount) in &sample.usage.metrics {
            self.add_metric(
                &format!("{prefix}/metric/{}", encode_segment(name)),
                *amount,
            )?;
        }
        self.cost = self
            .cost
            .checked_add(event_cost)
            .ok_or_else(|| overflow("cost"))?;
        self.usage.input_tokens = self
            .usage
            .input_tokens
            .checked_add(sample.usage.input_tokens)
            .ok_or_else(|| overflow("input tokens"))?;
        self.usage.output_tokens = self
            .usage
            .output_tokens
            .checked_add(sample.usage.output_tokens)
            .ok_or_else(|| overflow("output tokens"))?;
        self.usage.cached_input_tokens = self
            .usage
            .cached_input_tokens
            .checked_add(sample.usage.cached_input_tokens)
            .ok_or_else(|| overflow("cached input tokens"))?;
        for (name, amount) in sample.usage.metrics {
            let total = self.usage.metrics.entry(name).or_default();
            *total = total
                .checked_add(amount)
                .ok_or_else(|| overflow("dimensional usage"))?;
        }
        if sample.kind == SessionUsageKind::Transcription {
            self.usage
                .dimensions
                .insert("transcription_model".into(), sample.model);
        }
        Ok(())
    }

    pub(super) fn mark_compromised(&mut self) {
        self.usage
            .metrics
            .insert("realtime_meter_compromised".into(), Decimal::ONE);
    }

    fn add_metric(&mut self, name: &str, amount: Decimal) -> Result<(), CoreError> {
        if amount == Decimal::ZERO {
            return Ok(());
        }
        let total = self.usage.metrics.entry(name.into()).or_default();
        *total = total
            .checked_add(amount)
            .ok_or_else(|| overflow("model usage breakdown"))?;
        Ok(())
    }
}

fn encode_segment(value: &str) -> String {
    use std::fmt::Write as _;
    let mut output = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_') {
            output.push(char::from(byte));
        } else {
            write!(output, "%{byte:02X}").expect("writing to String");
        }
    }
    output
}

fn overflow(field: &str) -> CoreError {
    CoreError::Channel(gproxy_channel_api::ChannelError::Decode(format!(
        "Realtime {field} overflow"
    )))
}

pub(super) fn log_compromise(ctx: &FunnelCtx, error: &impl std::fmt::Display) {
    tracing::error!(request_id = %ctx.request_id, error = %error, "Realtime meter integrity was compromised");
}
