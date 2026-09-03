//! Stateful typed response-stream transforms.

use crate::TransformError;

pub trait TypedStreamTransform {
    type InputEvent;
    type OutputEvent;

    fn push(&mut self, event: Self::InputEvent) -> Result<Vec<Self::OutputEvent>, TransformError>;

    fn finish(&mut self) -> Result<Vec<Self::OutputEvent>, TransformError>;
}

pub mod openai_chat_to_gemini_generate_content {
    use gproxy_protocol::{gemini, openai};

    use super::{TransformError, TypedStreamTransform};

    #[derive(Default)]
    pub struct StreamTransform {
        inner: crate::generate_content::openai_chat_to_gemini_generate_content::stream::State,
    }

    impl TypedStreamTransform for StreamTransform {
        type InputEvent = gemini::GenerateContentResponse;
        type OutputEvent = openai::ChatCompletionChunk;

        fn push(
            &mut self,
            event: Self::InputEvent,
        ) -> Result<Vec<Self::OutputEvent>, TransformError> {
            self.inner.push_typed(event)
        }

        fn finish(&mut self) -> Result<Vec<Self::OutputEvent>, TransformError> {
            self.inner.finish_typed()
        }
    }
}

pub mod gemini_generate_content_to_openai_chat {
    use gproxy_protocol::{gemini, openai};

    use super::{TransformError, TypedStreamTransform};

    #[derive(Default)]
    pub struct StreamTransform {
        inner: crate::generate_content::gemini_generate_content_to_openai_chat::stream::State,
    }

    impl TypedStreamTransform for StreamTransform {
        type InputEvent = openai::ChatCompletionChunk;
        type OutputEvent = gemini::GenerateContentResponse;

        fn push(
            &mut self,
            event: Self::InputEvent,
        ) -> Result<Vec<Self::OutputEvent>, TransformError> {
            self.inner.push_typed(event)
        }

        fn finish(&mut self) -> Result<Vec<Self::OutputEvent>, TransformError> {
            self.inner.finish_typed()
        }
    }
}

pub mod claude_messages_to_gemini_generate_content {
    use gproxy_protocol::{claude, gemini};

    use super::{TransformError, TypedStreamTransform};

    #[derive(Default)]
    pub struct StreamTransform {
        inner: crate::generate_content::gemini_generate_content_to_claude_messages::stream::State,
    }

    impl TypedStreamTransform for StreamTransform {
        type InputEvent = gemini::GenerateContentResponse;
        type OutputEvent = claude::StreamEvent;

        fn push(
            &mut self,
            event: Self::InputEvent,
        ) -> Result<Vec<Self::OutputEvent>, TransformError> {
            self.inner.push_typed(event)
        }

        fn finish(&mut self) -> Result<Vec<Self::OutputEvent>, TransformError> {
            self.inner.finish_typed()
        }
    }
}

pub mod gemini_generate_content_to_claude_messages {
    use gproxy_protocol::{claude, gemini};

    use super::{TransformError, TypedStreamTransform};

    #[derive(Default)]
    pub struct StreamTransform {
        inner: crate::generate_content::claude_messages_to_gemini_generate_content::stream::State,
    }

    impl TypedStreamTransform for StreamTransform {
        type InputEvent = claude::StreamEvent;
        type OutputEvent = gemini::GenerateContentResponse;

        fn push(
            &mut self,
            event: Self::InputEvent,
        ) -> Result<Vec<Self::OutputEvent>, TransformError> {
            self.inner.push_typed(event)
        }

        fn finish(&mut self) -> Result<Vec<Self::OutputEvent>, TransformError> {
            self.inner.finish_typed()
        }
    }
}

pub mod openai_chat_to_openai_responses {
    use gproxy_protocol::openai;

    use super::{TransformError, TypedStreamTransform};

    #[derive(Default)]
    pub struct StreamTransform {
        inner: crate::generate_content::openai_chat_to_openai_responses::stream::State,
    }

    impl TypedStreamTransform for StreamTransform {
        type InputEvent = openai::ResponseStreamEvent;
        type OutputEvent = openai::ChatCompletionChunk;

        fn push(
            &mut self,
            event: Self::InputEvent,
        ) -> Result<Vec<Self::OutputEvent>, TransformError> {
            self.inner.push_typed(event)
        }

        fn finish(&mut self) -> Result<Vec<Self::OutputEvent>, TransformError> {
            self.inner.finish_typed()
        }
    }
}

pub mod openai_responses_to_openai_chat {
    use gproxy_protocol::openai;

    use super::{TransformError, TypedStreamTransform};

    #[derive(Default)]
    pub struct StreamTransform {
        inner: crate::generate_content::openai_responses_to_openai_chat::stream::State,
    }

    impl TypedStreamTransform for StreamTransform {
        type InputEvent = openai::ChatCompletionChunk;
        type OutputEvent = openai::ResponseStreamEvent;

        fn push(
            &mut self,
            event: Self::InputEvent,
        ) -> Result<Vec<Self::OutputEvent>, TransformError> {
            self.inner.push_typed(event)
        }

        fn finish(&mut self) -> Result<Vec<Self::OutputEvent>, TransformError> {
            self.inner.finish_typed()
        }
    }
}

pub mod openai_chat_to_claude_messages {
    use gproxy_protocol::{claude, openai};

    use super::{TransformError, TypedStreamTransform};
    use crate::common::stream::claude_to_openai::{Output, OutputEvent, State};

    pub struct StreamTransform {
        inner: State,
    }

    impl Default for StreamTransform {
        fn default() -> Self {
            Self {
                inner: State::new(Output::Chat),
            }
        }
    }

    impl TypedStreamTransform for StreamTransform {
        type InputEvent = claude::StreamEvent;
        type OutputEvent = openai::ChatCompletionChunk;

        fn push(
            &mut self,
            event: Self::InputEvent,
        ) -> Result<Vec<Self::OutputEvent>, TransformError> {
            chat(self.inner.push_typed(event)?)
        }

        fn finish(&mut self) -> Result<Vec<Self::OutputEvent>, TransformError> {
            chat(self.inner.finish_typed()?)
        }
    }

    fn chat(events: Vec<OutputEvent>) -> Result<Vec<openai::ChatCompletionChunk>, TransformError> {
        events
            .into_iter()
            .map(|event| match event {
                OutputEvent::Chat(event) => Ok(event),
                OutputEvent::Responses(_) => Err(TransformError::shape(
                    "typed stream",
                    "Claude-to-Chat converter emitted a Responses event",
                )),
            })
            .collect()
    }
}

pub mod openai_responses_to_claude_messages {
    use gproxy_protocol::{claude, openai};

    use super::{TransformError, TypedStreamTransform};
    use crate::common::stream::claude_to_openai::{Output, OutputEvent, State};

    pub struct StreamTransform {
        inner: State,
    }

    impl Default for StreamTransform {
        fn default() -> Self {
            Self {
                inner: State::new(Output::Responses),
            }
        }
    }

    impl TypedStreamTransform for StreamTransform {
        type InputEvent = claude::StreamEvent;
        type OutputEvent = openai::ResponseStreamEvent;

        fn push(
            &mut self,
            event: Self::InputEvent,
        ) -> Result<Vec<Self::OutputEvent>, TransformError> {
            responses(self.inner.push_typed(event)?)
        }

        fn finish(&mut self) -> Result<Vec<Self::OutputEvent>, TransformError> {
            responses(self.inner.finish_typed()?)
        }
    }

    fn responses(
        events: Vec<OutputEvent>,
    ) -> Result<Vec<openai::ResponseStreamEvent>, TransformError> {
        events
            .into_iter()
            .map(|event| match event {
                OutputEvent::Responses(event) => Ok(event),
                OutputEvent::Chat(_) => Err(TransformError::shape(
                    "typed stream",
                    "Claude-to-Responses converter emitted a Chat event",
                )),
            })
            .collect()
    }
}

pub mod claude_messages_to_openai_chat {
    use gproxy_protocol::{claude, openai};

    use super::{TransformError, TypedStreamTransform};
    use crate::common::stream::openai_to_claude::{Input, State};

    pub struct StreamTransform {
        inner: State,
    }

    impl Default for StreamTransform {
        fn default() -> Self {
            Self {
                inner: State::new(Input::Chat),
            }
        }
    }

    impl TypedStreamTransform for StreamTransform {
        type InputEvent = openai::ChatCompletionChunk;
        type OutputEvent = claude::StreamEvent;

        fn push(
            &mut self,
            event: Self::InputEvent,
        ) -> Result<Vec<Self::OutputEvent>, TransformError> {
            self.inner.push_chat_typed(event)
        }

        fn finish(&mut self) -> Result<Vec<Self::OutputEvent>, TransformError> {
            self.inner.finish_typed()
        }
    }
}

pub mod claude_messages_to_openai_responses {
    use gproxy_protocol::{claude, openai};

    use super::{TransformError, TypedStreamTransform};
    use crate::common::stream::openai_to_claude::{Input, State};

    pub struct StreamTransform {
        inner: State,
    }

    impl Default for StreamTransform {
        fn default() -> Self {
            Self {
                inner: State::new(Input::Responses),
            }
        }
    }

    impl TypedStreamTransform for StreamTransform {
        type InputEvent = openai::ResponseStreamEvent;
        type OutputEvent = claude::StreamEvent;

        fn push(
            &mut self,
            event: Self::InputEvent,
        ) -> Result<Vec<Self::OutputEvent>, TransformError> {
            self.inner.push_responses_typed(event)
        }

        fn finish(&mut self) -> Result<Vec<Self::OutputEvent>, TransformError> {
            self.inner.finish_typed()
        }
    }
}

pub mod gemini_generate_content_to_openai_responses {
    use gproxy_protocol::{gemini, openai};

    use super::{TransformError, TypedStreamTransform};

    pub struct StreamTransform {
        inner: crate::generate_content::gemini_generate_content_to_openai_responses::stream::State,
    }

    impl Default for StreamTransform {
        fn default() -> Self {
            Self {
                inner: crate::generate_content::gemini_generate_content_to_openai_responses::stream::State::new(),
            }
        }
    }

    impl TypedStreamTransform for StreamTransform {
        type InputEvent = openai::ResponseStreamEvent;
        type OutputEvent = gemini::GenerateContentResponse;

        fn push(
            &mut self,
            event: Self::InputEvent,
        ) -> Result<Vec<Self::OutputEvent>, TransformError> {
            self.inner.push_typed(event)
        }

        fn finish(&mut self) -> Result<Vec<Self::OutputEvent>, TransformError> {
            self.inner.finish_typed()
        }
    }
}

pub mod openai_responses_to_gemini_generate_content {
    use gproxy_protocol::{gemini, openai};

    use super::{TransformError, TypedStreamTransform};

    pub struct StreamTransform {
        inner: crate::generate_content::openai_responses_to_gemini_generate_content::stream::State,
    }

    impl Default for StreamTransform {
        fn default() -> Self {
            Self {
                inner: crate::generate_content::openai_responses_to_gemini_generate_content::stream::State::new(),
            }
        }
    }

    impl TypedStreamTransform for StreamTransform {
        type InputEvent = gemini::GenerateContentResponse;
        type OutputEvent = openai::ResponseStreamEvent;

        fn push(
            &mut self,
            event: Self::InputEvent,
        ) -> Result<Vec<Self::OutputEvent>, TransformError> {
            self.inner.push_typed(event)
        }

        fn finish(&mut self) -> Result<Vec<Self::OutputEvent>, TransformError> {
            self.inner.finish_typed()
        }
    }
}
