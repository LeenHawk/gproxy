//! Which media references are fetched before a cross-dialect conversion.
//! A reference the target reads as it stands is left in place; any other
//! is materialized, since the direct transforms drop or reject it.

use super::*;
use gproxy_protocol::{
    adapt::generate::reference_passes_through,
    capability::ResourceReference,
    transform::{
        Converted, TransformError,
        generate::{
            chat_responses, claude_chat, claude_gemini, claude_responses, gemini_chat,
            gemini_responses,
        },
        identity::{IdNamespace, IdentityFlow, TargetIdPolicy},
    },
    wire::{
        claude::generate_content as c,
        gemini as g,
        openai::{chat as h, responses as r},
    },
};

const MARK: &str = "REFMARK";
const PNG: &str = "iVBORw0KGgo";

#[derive(Clone, Copy)]
enum Source {
    Chat,
    Claude,
    Responses,
    Gemini,
}

impl Source {
    fn dialect(self) -> Dialect {
        match self {
            Self::Chat => Dialect::OpenAiChat,
            Self::Claude => Dialect::Claude,
            Self::Responses => Dialect::OpenAi,
            Self::Gemini => Dialect::Gemini,
        }
    }
}

/// One media reference in a source request: whether it is an image, and
/// whether it is written as a URL or a native ID.
struct Case {
    source: Source,
    image: bool,
    url: bool,
    content: Value,
}

fn cases() -> Vec<Case> {
    let image_url = format!("https://example.com/{MARK}.png");
    let file_url = format!("https://example.com/{MARK}.pdf");
    let id = format!("file_{MARK}");
    let case = |source, image, url, content| Case {
        source,
        image,
        url,
        content,
    };
    vec![
        case(
            Source::Claude,
            true,
            true,
            json!({"type":"image","source":{"type":"url","url":image_url}}),
        ),
        case(
            Source::Claude,
            true,
            false,
            json!({"type":"image","source":{"type":"file","file_id":id}}),
        ),
        case(
            Source::Claude,
            false,
            true,
            json!({"type":"document","source":{"type":"url","url":file_url}}),
        ),
        case(
            Source::Claude,
            false,
            false,
            json!({"type":"document","source":{"type":"file","file_id":id}}),
        ),
        case(
            Source::Chat,
            true,
            true,
            json!({"type":"image_url","image_url":{"url":image_url}}),
        ),
        case(
            Source::Chat,
            false,
            false,
            json!({"type":"file","file":{"file_id":id}}),
        ),
        case(
            Source::Responses,
            true,
            true,
            json!({"type":"input_image","detail":"auto","image_url":image_url}),
        ),
        case(
            Source::Responses,
            true,
            false,
            json!({"type":"input_image","detail":"auto","file_id":id}),
        ),
        case(
            Source::Responses,
            false,
            true,
            json!({"type":"input_file","file_url":file_url}),
        ),
        case(
            Source::Responses,
            false,
            false,
            json!({"type":"input_file","file_id":id}),
        ),
        case(
            Source::Gemini,
            true,
            true,
            json!({"fileData":{"mimeType":"image/png","fileUri":image_url}}),
        ),
        case(
            Source::Gemini,
            false,
            true,
            json!({"fileData":{"mimeType":"application/pdf","fileUri":file_url}}),
        ),
    ]
}

fn targets(source: Source) -> Vec<Dialect> {
    [
        Dialect::OpenAiChat,
        Dialect::Claude,
        Dialect::OpenAi,
        Dialect::Gemini,
    ]
    .into_iter()
    .filter(|target| *target != source.dialect())
    .collect()
}

fn request(source: Source, content: Value) -> Value {
    match source {
        Source::Claude => json!({"model":"m","max_tokens":10,"messages":[
            {"role":"user","content":[{"type":"text","text":"x"},content]}]}),
        Source::Chat => json!({"model":"m","max_completion_tokens":10,"messages":[
            {"role":"user","content":[{"type":"text","text":"x"},content]}]}),
        Source::Responses => json!({"model":"m","max_output_tokens":10,"input":[
            {"type":"message","role":"user","content":[{"type":"input_text","text":"x"},content]}]}),
        Source::Gemini => json!({"contents":[{"role":"user","parts":[{"text":"x"},content]}]}),
    }
}

fn flow() -> IdentityFlow {
    IdentityFlow::new(IdNamespace::with_bytes([3; 16]))
}

fn wire<T: serde::Serialize>(value: Result<Converted<T>, TransformError>) -> Option<String> {
    value
        .ok()
        .map(|converted| serde_json::to_string(&converted.value).unwrap())
}

/// The direct transform of `body` from `source` to `target`, serialized, or
/// `None` when it fails.
fn convert(source: Source, target: Dialect, body: Value) -> Option<String> {
    match (source, target) {
        (Source::Claude, Dialect::OpenAiChat) => wire(claude_chat::claude_to_openai(
            &serde_json::from_value::<c::GenerateContentRequestBody>(body).unwrap(),
            "t",
        )),
        (Source::Claude, Dialect::OpenAi) => wire(claude_responses::claude_to_responses_request(
            serde_json::from_value(body).unwrap(),
            "t",
            &mut flow(),
            &TargetIdPolicy::new(Dialect::OpenAi),
        )),
        (Source::Claude, Dialect::Gemini) => wire(claude_gemini::claude_to_gemini_request(
            serde_json::from_value(body).unwrap(),
            "t",
            Default::default(),
            &mut flow(),
            &TargetIdPolicy::new(Dialect::Gemini),
        )),
        (Source::Chat, Dialect::Claude) => wire(claude_chat::openai_to_claude(
            &serde_json::from_value::<h::GenerateContentRequestBody>(body).unwrap(),
            "t",
        )),
        (Source::Chat, Dialect::OpenAi) => wire(chat_responses::chat_to_responses_request(
            serde_json::from_value(body).unwrap(),
            "t",
            &mut flow(),
            &TargetIdPolicy::new(Dialect::OpenAi),
        )),
        (Source::Chat, Dialect::Gemini) => wire(gemini_chat::openai_to_gemini_request(
            &serde_json::from_value::<h::GenerateContentRequestBody>(body).unwrap(),
            "t",
            &Default::default(),
        )),
        (Source::Responses, Dialect::Claude) => {
            wire(claude_responses::responses_to_claude_request(
                serde_json::from_value::<r::GenerateContentRequestBody>(body).unwrap(),
                "t",
                Default::default(),
            ))
        }
        (Source::Responses, Dialect::OpenAiChat) => wire(
            chat_responses::responses_to_chat_request(serde_json::from_value(body).unwrap(), "t"),
        ),
        (Source::Responses, Dialect::Gemini) => {
            wire(gemini_responses::responses_to_gemini_request(
                serde_json::from_value(body).unwrap(),
                "t",
                Default::default(),
            ))
        }
        (Source::Gemini, Dialect::Claude) => wire(claude_gemini::gemini_to_claude_request(
            serde_json::from_value::<g::GenerateContentRequestBody>(body).unwrap(),
            "t",
            Some(10),
            &mut flow(),
            &TargetIdPolicy::new(Dialect::Claude),
        )),
        (Source::Gemini, Dialect::OpenAiChat) => wire(gemini_chat::gemini_to_openai_request(
            serde_json::from_value(body).unwrap(),
            "t",
            &mut flow(),
            &TargetIdPolicy::new(Dialect::OpenAiChat),
        )),
        (Source::Gemini, Dialect::OpenAi) => wire(gemini_responses::gemini_to_responses_request(
            serde_json::from_value(body).unwrap(),
            "t",
            &mut flow(),
            &TargetIdPolicy::new(Dialect::OpenAi),
        )),
        _ => unreachable!("targets exclude the source dialect"),
    }
}

fn reference(case: &Case) -> ResourceReference {
    if case.url {
        ResourceReference::Url(MARK.into())
    } else {
        ResourceReference::Id(MARK.into())
    }
}

#[test]
fn passthrough_rule_matches_what_direct_transforms_keep() {
    for case in cases() {
        for target in targets(case.source) {
            let body = request(case.source, case.content.clone());
            let kept = convert(case.source, target, body).is_some_and(|out| out.contains(MARK));
            let rule = reference_passes_through(
                case.source.dialect(),
                target,
                case.image,
                &reference(&case),
            );
            assert_eq!(
                kept,
                rule,
                "{} {} {} -> {}",
                case.source.dialect().id(),
                if case.image { "image" } else { "file" },
                if case.url { "url" } else { "id" },
                target.id()
            );
        }
    }
}

#[test]
fn image_references_the_target_cannot_read_are_fetched_and_reach_it() {
    for case in cases().into_iter().filter(|case| case.image) {
        for target in targets(case.source) {
            let access = Resources::png();
            let scope = "source".to_string();
            let resources = GenerationResources {
                target,
                ..resources(&access, &scope)
            };
            let body = request(case.source, case.content.clone());
            let materialized = match case.source {
                Source::Claude => serde_json::to_value(
                    ready(resources.claude(serde_json::from_value(body).unwrap())).unwrap(),
                ),
                Source::Chat => serde_json::to_value(
                    ready(resources.chat(serde_json::from_value(body).unwrap())).unwrap(),
                ),
                Source::Responses => serde_json::to_value(
                    ready(resources.responses(serde_json::from_value(body).unwrap())).unwrap(),
                ),
                Source::Gemini => serde_json::to_value(
                    ready(resources.gemini(serde_json::from_value(body).unwrap())).unwrap(),
                ),
            }
            .unwrap();
            let passes =
                reference_passes_through(case.source.dialect(), target, true, &reference(&case));
            let label = format!("{} -> {}", case.source.dialect().id(), target.id());
            assert_eq!(access.reads.lock().unwrap().is_empty(), passes, "{label}");
            let out = convert(case.source, target, materialized)
                .unwrap_or_else(|| panic!("{label} failed after materialization"));
            let expected = if passes { MARK } else { PNG };
            assert!(out.contains(expected), "{label}: {out}");
        }
    }
}
