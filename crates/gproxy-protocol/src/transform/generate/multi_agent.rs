//! Project hosted collaboration onto single-agent target dialects.
use crate::{
    transform::Report,
    wire::openai::responses::{input::InputItem, response::ResponseOutputItem},
};

pub(crate) fn omit_input(item: &InputItem, report: &mut Report) -> bool {
    let hosted = matches!(
        item,
        InputItem::MultiAgentCall(_)
            | InputItem::MultiAgentCallOutput(_)
            | InputItem::AgentMessage(_)
    );
    let child_content = item.agent().is_some_and(|v| v.agent_name != "/root")
        && matches!(
            item,
            InputItem::Message(_)
                | InputItem::Easy(_)
                | InputItem::OutputMessage(_)
                | InputItem::Reasoning(_)
                | InputItem::ConfigurationUpdate(_)
        );
    if hosted || child_content {
        report.omitted(
            "input.multi_agent",
            "hosted collaboration and subagent messages have no single-agent history equivalent",
        );
    } else if item.agent().is_some() {
        report.omitted(
            "input.agent",
            "target retains the item without OpenAI agent attribution",
        );
    }
    hosted || child_content
}

pub(crate) fn excluded_output(item: &ResponseOutputItem) -> bool {
    matches!(
        item,
        ResponseOutputItem::MultiAgentCall(_)
            | ResponseOutputItem::MultiAgentCallOutput(_)
            | ResponseOutputItem::AgentMessage(_)
    ) || item.agent().is_some_and(|v| v.agent_name != "/root")
        && matches!(
            item,
            ResponseOutputItem::Message(_) | ResponseOutputItem::Reasoning(_)
        )
}

pub(crate) fn omit_output(item: &ResponseOutputItem, report: &mut Report) -> bool {
    let excluded = excluded_output(item);
    if excluded {
        report.omitted(
            "output.multi_agent",
            "hosted collaboration and subagent text are not the root agent's answer",
        );
    } else if item.agent().is_some() {
        report.omitted(
            "output.agent",
            "target retains the item without OpenAI agent attribution",
        );
    }
    excluded
}

pub(crate) fn attribute_event(
    mut event: crate::wire::openai::responses::stream::StreamEvent,
) -> Result<crate::wire::openai::responses::stream::StreamEvent, crate::transform::TransformError> {
    use crate::wire::openai::responses::stream::StreamEvent;
    let agent = event.agent().cloned();
    if let Some(agent) = agent
        && let StreamEvent::OutputItemAdded(v) | StreamEvent::OutputItemDone(v) = &mut event
    {
        if v.item.agent().is_some_and(|owner| owner != &agent) {
            return Err(crate::transform::TransformError::invalid_result(
                "output.agent",
                "event and item identify different agents",
            ));
        }
        v.item.set_agent(agent);
    }
    Ok(event)
}
