mod collector;
mod message;
mod output;
mod reasoning;
mod sanitize;
mod tool;

use crate::protocol::openai;

use collector::ResponseCollector;

pub fn response(
    events: impl IntoIterator<Item = openai::ResponseStreamEvent>,
) -> openai::ResponseObject {
    let mut collector = ResponseCollector::default();
    for event in events {
        collector.push(event);
    }
    collector.finish()
}
