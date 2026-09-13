use crate::{
    transform::{Converted, Report, TransformError},
    wire::{claude::count_tokens as c, gemini::count_tokens as g, openai::count_tokens as o},
};

/// Facts from the original Claude counting request and any context-edit pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaudeCountContext {
    /// The counted input is identical to the original input: no edits occurred.
    Unedited,
    /// An actual count obtained before applying context management.
    OriginalInputTokens(i64),
    /// Edits were requested, but their original token count is unavailable.
    Unknown,
}

impl ClaudeCountContext {
    pub fn for_request(request: &c::CountTokensRequestBody) -> Self {
        if request
            .context_management
            .as_ref()
            .and_then(|config| config.edits.as_ref())
            .is_none_or(Vec::is_empty)
        {
            Self::Unedited
        } else {
            Self::Unknown
        }
    }

    fn original(self, counted: i64) -> Result<i64, TransformError> {
        match self {
            Self::Unedited => Ok(counted),
            Self::OriginalInputTokens(value) => {
                nonnegative(value, "context_management.original_input_tokens")
            }
            Self::Unknown => Err(TransformError::missing_metadata(
                "context_management.original_input_tokens",
            )),
        }
    }
}

pub fn openai_to_claude_response(
    input: o::CountTokensResponseBody,
    context: ClaudeCountContext,
) -> Result<Converted<c::CountTokensResponseBody>, TransformError> {
    claude(input.input_tokens, context, Report::default())
}

pub fn gemini_to_claude_response(
    input: g::CountTokensResponseBody,
    context: ClaudeCountContext,
) -> Result<Converted<c::CountTokensResponseBody>, TransformError> {
    let (count, report) = gemini_count(&input)?;
    claude(count, context, report)
}

pub fn claude_to_openai_response(
    input: c::CountTokensResponseBody,
) -> Result<Converted<o::CountTokensResponseBody>, TransformError> {
    let report = claude_report(&input)?;
    openai(input.input_tokens, report)
}

pub fn gemini_to_openai_response(
    input: g::CountTokensResponseBody,
) -> Result<Converted<o::CountTokensResponseBody>, TransformError> {
    let (count, report) = gemini_count(&input)?;
    openai(count, report)
}

pub fn openai_to_gemini_response(
    input: o::CountTokensResponseBody,
) -> Result<Converted<g::CountTokensResponseBody>, TransformError> {
    gemini(input.input_tokens, Report::default())
}

pub fn claude_to_gemini_response(
    input: c::CountTokensResponseBody,
) -> Result<Converted<g::CountTokensResponseBody>, TransformError> {
    let report = claude_report(&input)?;
    gemini(input.input_tokens, report)
}

fn claude(
    count: i64,
    context: ClaudeCountContext,
    report: Report,
) -> Result<Converted<c::CountTokensResponseBody>, TransformError> {
    let count = nonnegative(count, "input_tokens")?;
    let original = context.original(count)?;
    Ok(Converted {
        value: c::CountTokensResponseBody::builder(
            c::CountTokensContextManagementResponse::builder(original).build(),
            count,
        )
        .build(),
        report,
    })
}

fn openai(
    count: i64,
    report: Report,
) -> Result<Converted<o::CountTokensResponseBody>, TransformError> {
    Ok(Converted {
        value: o::CountTokensResponseBody::builder(
            nonnegative(count, "input_tokens")?,
            o::CountTokensObject::ResponseInputTokens,
        )
        .build(),
        report,
    })
}

fn gemini(
    count: i64,
    report: Report,
) -> Result<Converted<g::CountTokensResponseBody>, TransformError> {
    Ok(Converted {
        value: g::CountTokensResponseBody::builder()
            .total_tokens(nonnegative(count, "input_tokens")?)
            .build(),
        report,
    })
}

fn claude_report(input: &c::CountTokensResponseBody) -> Result<Report, TransformError> {
    nonnegative(
        input.context_management.original_input_tokens,
        "context_management.original_input_tokens",
    )?;
    let mut report = Report::default();
    report.omitted(
        "context_management.original_input_tokens",
        "target reports only the counted input, without the count before context edits",
    );
    Ok(report)
}

fn gemini_count(input: &g::CountTokensResponseBody) -> Result<(i64, Report), TransformError> {
    let count = nonnegative(
        input
            .total_tokens
            .ok_or_else(|| TransformError::missing_metadata("totalTokens"))?,
        "totalTokens",
    )?;
    let mut report = Report::default();
    if let Some(cached) = input.cached_content_token_count {
        nonnegative(cached, "cachedContentTokenCount")?;
        if cached > count {
            return Err(TransformError::invalid_result(
                "cachedContentTokenCount",
                "cached tokens exceed total input tokens",
            ));
        }
        report.omitted(
            "cachedContentTokenCount",
            "target count result has no cached-token detail",
        );
    }
    for (field, values) in [
        ("promptTokensDetails", &input.prompt_tokens_details),
        ("cacheTokensDetails", &input.cache_tokens_details),
    ] {
        if let Some(values) = values {
            for (index, value) in values.iter().enumerate() {
                if let Some(count) = value.token_count {
                    nonnegative(count, &format!("{field}[{index}].tokenCount"))?;
                }
            }
            if !values.is_empty() {
                report.omitted(field, "target count result has no modality-token detail");
            }
        }
    }
    Ok((count, report))
}

fn nonnegative(count: i64, field: &str) -> Result<i64, TransformError> {
    if count < 0 {
        return Err(TransformError::invalid_result(
            field,
            "token count cannot be negative",
        ));
    }
    Ok(count)
}
