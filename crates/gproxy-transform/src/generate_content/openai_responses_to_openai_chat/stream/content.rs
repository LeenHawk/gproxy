use gproxy_protocol::openai;

use super::{Item, Scalar};

pub(super) fn item(
    item: &Item,
    status: openai::ResponseItemLifecycleStatus,
) -> openai::ResponseItem {
    match item.kind {
        Scalar::Reasoning => {
            openai::ResponseItem::Typed(Box::new(openai::TypedResponseItem::Reasoning {
                id: Some(item.id.clone()),
                summary: Vec::new(),
                content: Some(vec![crate::wire!(openai::ResponseReasoningTextPart {
                    text: item.text.clone(),
                    type_: openai::ResponseReasoningTextType::ReasoningText,
                    rest: Default::default(),
                })]),
                encrypted_content: None,
                status: Some(status),
                rest: Default::default(),
            }))
        }
        Scalar::Text => message(
            item,
            status,
            openai::ResponseMessageOutputContentPart::OutputText(text(item)),
        ),
        Scalar::Refusal => message(
            item,
            status,
            openai::ResponseMessageOutputContentPart::Refusal(refusal(item)),
        ),
    }
}

fn message(
    item: &Item,
    status: openai::ResponseItemLifecycleStatus,
    part: openai::ResponseMessageOutputContentPart,
) -> openai::ResponseItem {
    openai::ResponseItem::Message(openai::ResponseMessageItem::Output(crate::wire!(
        openai::ResponseOutputMessageItem {
            type_: openai::ResponseMessageItemType::Message,
            id: item.id.clone(),
            role: openai::ResponseOutputMessageRole::Assistant,
            content: vec![part],
            status,
            phase: None,
            rest: Default::default(),
        }
    )))
}

pub(super) fn part(item: &Item) -> openai::ResponseContentPart {
    match item.kind {
        Scalar::Text => openai::ResponseContentPart::OutputText(text(item)),
        Scalar::Refusal => openai::ResponseContentPart::Refusal(refusal(item)),
        Scalar::Reasoning => openai::ResponseContentPart::ReasoningText(crate::wire!(
            openai::ResponseReasoningText {
                type_: openai::ResponseReasoningTextType::ReasoningText,
                text: item.text.clone(),
                rest: Default::default(),
            }
        )),
    }
}

fn text(item: &Item) -> openai::ResponseOutputText {
    crate::wire!(openai::ResponseOutputText {
        type_: openai::ResponseOutputTextType::OutputText,
        annotations: Vec::new(),
        logprobs: (!item.logprobs.is_empty()).then(|| item.logprobs.clone()),
        text: item.text.clone(),
        rest: Default::default(),
    })
}

fn refusal(item: &Item) -> openai::ResponseRefusal {
    crate::wire!(openai::ResponseRefusal {
        type_: openai::ResponseRefusalType::Refusal,
        refusal: item.text.clone(),
        rest: Default::default(),
    })
}
