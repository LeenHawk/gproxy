use serde_json::json;

use crate::openai::generate_content::responses::{
    ApplyPatchOperation, CodeInterpreterOutput, ComputerAction, ComputerScreenshot,
    ComputerScreenshotType, LocalShellAction, LocalShellActionType, McpToolDescription,
    ResponseAnnotation, ResponseReasoningSummaryPart, ResponseReasoningSummaryType,
    ResponseReasoningTextPart, ResponseReasoningTextType, ShellCallOutcome, ShellEnvironment,
    WebSearchAction, WebSearchSourceType,
};

use super::round_trip;

type ItemActionFixture = (
    Vec<ResponseAnnotation>,
    Vec<ComputerAction>,
    ComputerScreenshot,
    Vec<WebSearchAction>,
    ResponseReasoningSummaryPart,
    ResponseReasoningTextPart,
    Vec<CodeInterpreterOutput>,
    LocalShellAction,
    Vec<ShellEnvironment>,
    Vec<ShellCallOutcome>,
    Vec<ApplyPatchOperation>,
    McpToolDescription,
);

#[test]
fn item_action_unions_round_trip() {
    let fixture = round_trip::<ItemActionFixture>(json!([
        [
            {"type":"file_citation","file_id":"file_1","filename":"a.txt","index":0,"future":1},
            {"type":"url_citation","end_index":9,"start_index":1,"title":"Source","url":"https://example.com","future":2},
            {"type":"container_file_citation","container_id":"ctr_1","end_index":9,"file_id":"file_2","filename":"b.txt","start_index":1,"future":3},
            {"type":"file_path","file_id":"file_3","index":2,"future":4}
        ],
        [
            {"type":"click","button":"left","x":1.0,"y":2.0,"future":1},
            {"type":"double_click","keys":[],"x":2.0,"y":3.0,"future":2},
            {"type":"drag","path":[{"x":1.0,"y":2.0,"point_future":true}],"future":3},
            {"type":"keypress","keys":["ENTER"],"future":4},
            {"type":"move","x":4.0,"y":5.0,"future":5},
            {"type":"screenshot","future":6},
            {"type":"scroll","scroll_x":0.0,"scroll_y":10.0,"x":4.0,"y":5.0,"future":7},
            {"type":"type","text":"hello","future":8},
            {"type":"wait","future":9}
        ],
        {"type":"computer_screenshot","file_id":"file_4","screenshot_future":true},
        [
            {"type":"search","query":"rust","sources":[{"type":"url","url":"https://example.com","source_future":true}],"future":1},
            {"type":"open_page","future":2},
            {"type":"find_in_page","pattern":"serde","url":"https://example.com","future":3}
        ],
        {"type":"summary_text","text":"summary","future":1},
        {"type":"reasoning_text","text":"reasoning","future":2},
        [
            {"type":"logs","logs":"ok","future":1},
            {"type":"image","url":"https://example.com/image.png","future":2}
        ],
        {"type":"exec","command":["pwd"],"env":{},"timeout_ms":1000,"future":1},
        [
            {"type":"local","skills":[{"skill_id":"skill_1","version":"latest","skill_future":true}],"future":1},
            {"type":"container_reference","container_id":"ctr_1","future":2}
        ],
        [
            {"type":"timeout","future":1},
            {"type":"exit","exit_code":0,"future":2}
        ],
        [
            {"type":"create_file","diff":"+new","path":"a.txt","future":1},
            {"type":"delete_file","path":"b.txt","future":2},
            {"type":"update_file","diff":"-old\n+new","path":"c.txt","future":3}
        ],
        {"input_schema":{"type":"object"},"name":"lookup","annotations":{"readOnlyHint":true},"mcp_future":true}
    ]));

    let (
        annotations,
        computer,
        screenshot,
        web,
        summary,
        reasoning,
        code,
        local,
        shell,
        outcomes,
        patch,
        mcp,
    ) = fixture;
    assert!(matches!(
        annotations.as_slice(),
        [
            ResponseAnnotation::FileCitation { .. },
            ResponseAnnotation::UrlCitation { .. },
            ResponseAnnotation::ContainerFileCitation { .. },
            ResponseAnnotation::FilePath { .. }
        ]
    ));
    assert!(matches!(
        computer.as_slice(),
        [
            ComputerAction::Click { .. },
            ComputerAction::DoubleClick { .. },
            ComputerAction::Drag { .. },
            ComputerAction::Keypress { .. },
            ComputerAction::Move { .. },
            ComputerAction::Screenshot { .. },
            ComputerAction::Scroll { .. },
            ComputerAction::Type { .. },
            ComputerAction::Wait { .. }
        ]
    ));
    assert!(matches!(
        screenshot.type_,
        ComputerScreenshotType::ComputerScreenshot
    ));
    assert!(matches!(
        web.as_slice(),
        [
            WebSearchAction::Search { .. },
            WebSearchAction::OpenPage { .. },
            WebSearchAction::FindInPage { .. }
        ]
    ));
    let WebSearchAction::Search {
        sources: Some(sources),
        ..
    } = &web[0]
    else {
        panic!("expected search sources")
    };
    assert!(matches!(sources[0].type_, WebSearchSourceType::Url));
    assert!(matches!(
        summary.type_,
        ResponseReasoningSummaryType::SummaryText
    ));
    assert!(matches!(
        reasoning.type_,
        ResponseReasoningTextType::ReasoningText
    ));
    assert!(matches!(
        code.as_slice(),
        [
            CodeInterpreterOutput::Logs { .. },
            CodeInterpreterOutput::Image { .. }
        ]
    ));
    assert!(matches!(local.type_, LocalShellActionType::Exec));
    assert!(matches!(
        shell.as_slice(),
        [
            ShellEnvironment::Local { .. },
            ShellEnvironment::ContainerReference { .. }
        ]
    ));
    assert!(matches!(
        outcomes.as_slice(),
        [
            ShellCallOutcome::Timeout { .. },
            ShellCallOutcome::Exit { .. }
        ]
    ));
    assert!(matches!(
        patch.as_slice(),
        [
            ApplyPatchOperation::CreateFile { .. },
            ApplyPatchOperation::DeleteFile { .. },
            ApplyPatchOperation::UpdateFile { .. }
        ]
    ));
    assert!(mcp.annotations.is_some());
}
