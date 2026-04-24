use std::collections::HashMap;

use crate::openai::count_tokens::types as ot;
use crate::openai::create_image::stream::ImageGenerationStreamEvent;
use crate::openai::create_response::response::ResponseBody;
use crate::openai::create_response::stream::ResponseStreamEvent;
use crate::openai::create_response::types as rt;
use crate::transform::openai::create_image::utils::{
    best_effort_image_usage_from_response_usage, stream_background_from_response_config,
    stream_error_from_response_error, stream_output_format_from_response_config,
    stream_quality_from_response_config_for_create_image,
    stream_size_from_response_config_for_create_image,
};

/// Stateful converter: OpenAI Responses API stream events → OpenAI Image
/// Generation stream events.
///
/// Accumulates image results from `OutputItemAdded`/`OutputItemDone` events and
/// tool configuration from response state events, then emits
/// `image_generation.partial_image` and `image_generation.completed` as images
/// become available.
#[derive(Debug, Clone, Default)]
pub struct ResponseStreamToImageStream {
    created_at: u64,
    background: Option<ot::ResponseImageGenerationBackground>,
    output_format: Option<ot::ResponseImageGenerationOutputFormat>,
    quality: Option<ot::ResponseImageGenerationQuality>,
    size: Option<ot::ResponseImageGenerationSize>,
    usage: Option<rt::ResponseUsage>,
    /// Accumulated image b64 results by item_id.
    results: HashMap<String, String>,
    /// Partial images emitted so far (for partial_image_index).
    partial_count: u32,
    finished: bool,
}

impl ResponseStreamToImageStream {
    fn update_config_from_response(&mut self, response: &ResponseBody) {
        self.created_at = response.created_at;
        if let Some(usage) = response.usage.as_ref() {
            self.usage = Some(usage.clone());
        }

        for tool in &response.tools {
            let rt::ResponseTool::ImageGeneration(image_tool) = tool else {
                continue;
            };
            if let Some(ref bg) = image_tool.background {
                self.background = Some(bg.clone());
            }
            if let Some(ref fmt) = image_tool.output_format {
                self.output_format = Some(fmt.clone());
            }
            if let Some(ref q) = image_tool.quality {
                self.quality = Some(q.clone());
            }
            if let Some(ref s) = image_tool.size {
                self.size = Some(s.clone());
            }
        }
    }

    fn collect_image_result(&mut self, item: &rt::ResponseOutputItem) {
        let rt::ResponseOutputItem::ImageGenerationCall(call) = item else {
            return;
        };
        if let Some(result) = call.result.as_deref().filter(|s| !s.is_empty()) {
            self.results.insert(call.id.clone(), result.to_string());
        }
    }

    fn emit_partial(&mut self, b64_json: String, out: &mut Vec<ImageGenerationStreamEvent>) {
        let index = self.partial_count;
        self.partial_count += 1;
        out.push(ImageGenerationStreamEvent::PartialImage {
            b64_json,
            background: stream_background_from_response_config(self.background.as_ref()),
            created_at: self.created_at,
            output_format: stream_output_format_from_response_config(self.output_format.as_ref()),
            partial_image_index: index,
            quality: stream_quality_from_response_config_for_create_image(self.quality.as_ref()),
            size: stream_size_from_response_config_for_create_image(self.size.as_ref()),
        });
    }

    fn emit_completed(&mut self, b64_json: String, out: &mut Vec<ImageGenerationStreamEvent>) {
        out.push(ImageGenerationStreamEvent::Completed {
            b64_json,
            background: stream_background_from_response_config(self.background.as_ref()),
            created_at: self.created_at,
            output_format: stream_output_format_from_response_config(self.output_format.as_ref()),
            quality: stream_quality_from_response_config_for_create_image(self.quality.as_ref()),
            size: stream_size_from_response_config_for_create_image(self.size.as_ref()),
            usage: best_effort_image_usage_from_response_usage(self.usage.as_ref()),
        });
    }

    pub fn on_event(
        &mut self,
        event: ResponseStreamEvent,
        out: &mut Vec<ImageGenerationStreamEvent>,
    ) {
        if self.finished {
            return;
        }

        match event {
            // State events — extract config and accumulate results
            ResponseStreamEvent::Created { response, .. }
            | ResponseStreamEvent::Queued { response, .. }
            | ResponseStreamEvent::InProgress { response, .. } => {
                self.update_config_from_response(&response);
                for item in &response.output {
                    self.collect_image_result(item);
                }
            }

            // Output item events — collect image results
            ResponseStreamEvent::OutputItemAdded { item, .. }
            | ResponseStreamEvent::OutputItemDone { item, .. } => {
                self.collect_image_result(&item);
            }

            // Partial image from Responses API streaming
            ResponseStreamEvent::ImageGenerationCallPartialImage {
                partial_image_b64, ..
            } => {
                self.emit_partial(partial_image_b64, out);
            }

            // Image generation completed — the result is in the item
            ResponseStreamEvent::ImageGenerationCallCompleted { item_id, .. } => {
                if let Some(b64) = self.results.remove(&item_id) {
                    // Will be emitted as Completed in finish()
                    self.results.insert(item_id, b64);
                }
            }

            // Completed — finalize
            ResponseStreamEvent::Completed { response, .. } => {
                self.update_config_from_response(&response);
                for item in &response.output {
                    self.collect_image_result(item);
                }
                self.finalize(out);
            }

            // Incomplete — finalize what we have
            ResponseStreamEvent::Incomplete { response, .. } => {
                self.update_config_from_response(&response);
                for item in &response.output {
                    self.collect_image_result(item);
                }
                self.finalize(out);
            }

            // Failed
            ResponseStreamEvent::Failed { response, .. } => {
                self.update_config_from_response(&response);
                let message = response
                    .error
                    .map(|e| e.message)
                    .unwrap_or_else(|| "image generation failed".to_string());
                out.push(ImageGenerationStreamEvent::Error {
                    error: stream_error_from_response_error(None, message, None),
                });
                self.finished = true;
            }

            // Error
            ResponseStreamEvent::Error { error, .. } => {
                out.push(ImageGenerationStreamEvent::Error {
                    error: stream_error_from_response_error(error.code, error.message, error.param),
                });
                self.finished = true;
            }

            // All other events are ignored (text deltas, reasoning, etc.)
            _ => {}
        }
    }

    fn finalize(&mut self, out: &mut Vec<ImageGenerationStreamEvent>) {
        if self.finished {
            return;
        }
        self.finished = true;

        // Emit Completed for each collected image result
        let results = std::mem::take(&mut self.results);
        for (_item_id, b64) in results {
            self.emit_completed(b64, out);
        }
    }

    pub fn finish(&mut self, out: &mut Vec<ImageGenerationStreamEvent>) {
        self.finalize(out);
    }
}
