//! Content-generation pairwise transforms.

#[allow(dead_code, unused_imports)]
mod common;

pub mod claude_messages_to_gemini_generate_content;
pub mod claude_messages_to_openai_chat;
pub mod claude_messages_to_openai_responses;
pub mod gemini_generate_content_to_claude_messages;
pub mod gemini_generate_content_to_openai_chat;
pub mod gemini_generate_content_to_openai_responses;
pub mod openai_chat_to_claude_messages;
pub mod openai_chat_to_gemini_generate_content;
pub mod openai_chat_to_openai_responses;
pub mod openai_responses_to_claude_messages;
pub mod openai_responses_to_gemini_generate_content;
pub mod openai_responses_to_openai_chat;
pub mod stream_to_response;
