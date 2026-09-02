use gproxy_protocol::openai::Rest;
use gproxy_protocol::openai::generate_content::responses::{
    KnownResponseStreamEvent as Known, ResponseCustomToolCallInputDoneEvent,
    ResponseFunctionCallArgumentsDoneEvent, ResponseItem, ResponseObject, ResponseOutputItemEvent,
    ResponseStreamEvent,
};

pub(super) fn response(event: &Known) -> Option<&ResponseObject> {
    match event {
        Known::ResponseCreated(event)
        | Known::ResponseInProgress(event)
        | Known::ResponseCompleted(event)
        | Known::ResponseFailed(event)
        | Known::ResponseIncomplete(event)
        | Known::ResponseQueued(event) => Some(&event.response),
        Known::ResponseInjectCreated(_)
        | Known::ResponseInjectFailed(_)
        | Known::ResponseOutputItemAdded(_)
        | Known::ResponseOutputItemDone(_)
        | Known::ResponseContentPartAdded(_)
        | Known::ResponseContentPartDone(_)
        | Known::ResponseOutputTextDelta(_)
        | Known::ResponseOutputTextDone(_)
        | Known::ResponseOutputTextAnnotationAdded(_)
        | Known::ResponseFunctionCallArgumentsDelta(_)
        | Known::ResponseFunctionCallArgumentsDone(_)
        | Known::ResponseCustomToolCallInputDelta(_)
        | Known::ResponseCustomToolCallInputDone(_)
        | Known::ResponseRefusalDelta(_)
        | Known::ResponseRefusalDone(_)
        | Known::ResponseReasoningSummaryPartAdded(_)
        | Known::ResponseReasoningSummaryPartDone(_)
        | Known::ResponseReasoningSummaryTextDelta(_)
        | Known::ResponseReasoningSummaryTextDone(_)
        | Known::ResponseReasoningTextDelta(_)
        | Known::ResponseReasoningTextDone(_)
        | Known::ResponseAudioDelta(_)
        | Known::ResponseAudioDone(_)
        | Known::ResponseAudioTranscriptDelta(_)
        | Known::ResponseAudioTranscriptDone(_)
        | Known::ResponseImageGenerationCallCompleted(_)
        | Known::ResponseImageGenerationCallGenerating(_)
        | Known::ResponseImageGenerationCallInProgress(_)
        | Known::ResponseImageGenerationCallPartialImage(_)
        | Known::ResponseFileSearchCallInProgress(_)
        | Known::ResponseFileSearchCallSearching(_)
        | Known::ResponseFileSearchCallCompleted(_)
        | Known::ResponseWebSearchCallInProgress(_)
        | Known::ResponseWebSearchCallSearching(_)
        | Known::ResponseWebSearchCallCompleted(_)
        | Known::ResponseCodeInterpreterCallInProgress(_)
        | Known::ResponseCodeInterpreterCallInterpreting(_)
        | Known::ResponseCodeInterpreterCallCompleted(_)
        | Known::ResponseCodeInterpreterCallCodeDelta(_)
        | Known::ResponseCodeInterpreterCallCodeDone(_)
        | Known::ResponseMcpCallArgumentsDelta(_)
        | Known::ResponseMcpCallArgumentsDone(_)
        | Known::ResponseMcpCallInProgress(_)
        | Known::ResponseMcpCallCompleted(_)
        | Known::ResponseMcpCallFailed(_)
        | Known::ResponseMcpListToolsInProgress(_)
        | Known::ResponseMcpListToolsCompleted(_)
        | Known::ResponseMcpListToolsFailed(_)
        | Known::Error(_) => None,
    }
}

pub(super) fn output_item_added(
    output_index: u32,
    item: ResponseItem,
    sequence_number: Option<u64>,
    rest: Rest,
) -> ResponseStreamEvent {
    known(Known::ResponseOutputItemAdded(ResponseOutputItemEvent {
        item: Box::new(item),
        output_index,
        sequence_number,
        rest,
    }))
}

pub(super) fn output_item_done(output_index: u32, item: ResponseItem) -> ResponseStreamEvent {
    known(Known::ResponseOutputItemDone(ResponseOutputItemEvent {
        item: Box::new(item),
        output_index,
        sequence_number: None,
        rest: Default::default(),
    }))
}

pub(super) fn function_arguments_done(
    output_index: u32,
    item_id: Option<String>,
    name: Option<String>,
    arguments: String,
) -> ResponseStreamEvent {
    known(Known::ResponseFunctionCallArgumentsDone(
        ResponseFunctionCallArgumentsDoneEvent {
            arguments,
            item_id,
            name,
            output_index,
            sequence_number: None,
            rest: Default::default(),
        },
    ))
}

pub(super) fn custom_input_done(
    output_index: u32,
    item_id: String,
    input: String,
) -> ResponseStreamEvent {
    known(Known::ResponseCustomToolCallInputDone(
        ResponseCustomToolCallInputDoneEvent {
            input,
            item_id,
            output_index,
            sequence_number: None,
            rest: Default::default(),
        },
    ))
}

fn known(event: Known) -> ResponseStreamEvent {
    ResponseStreamEvent::Known(Box::new(event))
}
