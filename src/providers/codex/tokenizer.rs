use axum::http::StatusCode;
use tiktoken_rs::{CoreBPE, cl100k_base, get_bpe_from_model, o200k_base};

use crate::formats::openai::responses_input_tokens::ResponseInputTokensRequest;
use crate::formats::openai::types::{
    EasyInputMessageContent, InputContent, InputItem, InputMessage, Item, OutputMessage,
    OutputMessageContent, ToolCallOutput, ToolCallOutputContentParam,
};

pub(crate) fn count_response_input_tokens(
    req: &ResponseInputTokensRequest,
) -> Result<i64, StatusCode> {
    let bpe = bpe_for_model(req.model.as_deref())?;
    let mut total = 0i64;

    if let Some(input) = &req.input {
        total += count_input_param(input, &bpe);
    }
    if let Some(instructions) = &req.instructions {
        total += count_text(instructions, &bpe);
    }

    Ok(total)
}

fn bpe_for_model(model: Option<&str>) -> Result<CoreBPE, StatusCode> {
    if let Some(model) = model {
        if let Ok(bpe) = get_bpe_from_model(model) {
            return Ok(bpe);
        }
        if is_o200k_model(model) {
            return o200k_base().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR);
        }
    }
    cl100k_base().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

fn is_o200k_model(model: &str) -> bool {
    model.starts_with("gpt-5")
        || model.starts_with("gpt-4.1")
        || model.starts_with("gpt-4o")
        || model.starts_with("o1")
        || model.starts_with("o3")
        || model.starts_with("o4")
}

fn count_input_param(input: &crate::formats::openai::types::InputParam, bpe: &CoreBPE) -> i64 {
    match input {
        crate::formats::openai::types::InputParam::Text(text) => count_text(text, bpe),
        crate::formats::openai::types::InputParam::Items(items) => {
            items.iter().map(|item| count_input_item(item, bpe)).sum()
        }
    }
}

fn count_input_item(item: &InputItem, bpe: &CoreBPE) -> i64 {
    match item {
        InputItem::EasyMessage(message) => count_easy_message(&message.content, bpe),
        InputItem::Item(item) => count_item(item, bpe),
        InputItem::Reference(_) => 0,
    }
}

fn count_easy_message(content: &EasyInputMessageContent, bpe: &CoreBPE) -> i64 {
    match content {
        EasyInputMessageContent::Text(text) => count_text(text, bpe),
        EasyInputMessageContent::Parts(parts) => parts
            .iter()
            .map(|part| count_input_content(part, bpe))
            .sum(),
    }
}

fn count_item(item: &Item, bpe: &CoreBPE) -> i64 {
    match item {
        Item::InputMessage(message) => count_input_message(message, bpe),
        Item::OutputMessage(message) => count_output_message(message, bpe),
        Item::FunctionCallOutputItem(output) => count_function_call_output(&output.output, bpe),
        Item::CustomToolCallOutput(output) => count_tool_call_output(&output.output, bpe),
        _ => 0,
    }
}

fn count_input_message(message: &InputMessage, bpe: &CoreBPE) -> i64 {
    message
        .content
        .iter()
        .map(|part| count_input_content(part, bpe))
        .sum()
}

fn count_output_message(message: &OutputMessage, bpe: &CoreBPE) -> i64 {
    message
        .content
        .iter()
        .map(|part| match part {
            OutputMessageContent::OutputText(text) => count_text(&text.text, bpe),
            OutputMessageContent::Refusal(refusal) => count_text(&refusal.refusal, bpe),
        })
        .sum()
}

fn count_function_call_output(
    output: &crate::formats::openai::types::FunctionCallOutputParam,
    bpe: &CoreBPE,
) -> i64 {
    match output {
        crate::formats::openai::types::FunctionCallOutputParam::Text(text) => {
            count_text(text, bpe)
        }
        crate::formats::openai::types::FunctionCallOutputParam::Parts(parts) => parts
            .iter()
            .map(|part| count_tool_call_output_content(part, bpe))
            .sum(),
    }
}

fn count_tool_call_output(output: &ToolCallOutput, bpe: &CoreBPE) -> i64 {
    match output {
        ToolCallOutput::Text(text) => count_text(text, bpe),
        ToolCallOutput::Parts(parts) => parts
            .iter()
            .map(|part| count_input_content(part, bpe))
            .sum(),
    }
}

fn count_tool_call_output_content(part: &ToolCallOutputContentParam, bpe: &CoreBPE) -> i64 {
    match part {
        ToolCallOutputContentParam::InputText { text } => count_text(text, bpe),
        ToolCallOutputContentParam::InputImage { .. } => 0,
        ToolCallOutputContentParam::InputFile { .. } => 0,
    }
}

fn count_input_content(content: &InputContent, bpe: &CoreBPE) -> i64 {
    match content {
        InputContent::InputText { text } => count_text(text, bpe),
        InputContent::InputImage { .. } => 0,
        InputContent::InputFile { .. } => 0,
    }
}

fn count_text(text: &str, bpe: &CoreBPE) -> i64 {
    bpe.encode_ordinary(text).len() as i64
}
