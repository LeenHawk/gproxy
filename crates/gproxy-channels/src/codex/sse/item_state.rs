use gproxy_protocol::openai::generate_content::responses::{
    ResponseItem, ResponseMessageItem, TypedResponseItem,
};

pub(super) fn clear_started_payload(item: &mut ResponseItem) {
    match item {
        ResponseItem::Message(ResponseMessageItem::Output(message)) => message.content.clear(),
        ResponseItem::Typed(item) => {
            if let TypedResponseItem::Reasoning {
                summary, content, ..
            } = item.as_mut()
            {
                summary.clear();
                *content = None;
            }
        }
        _ => {}
    }
}
