use super::*;
use crate::media_support::{
    self as shared,
    io::{self, Head, Input},
};
use base64::Engine;
use futures_util::StreamExt;
use protocol::openai::images as o;
pub type ImageStream = MediaStream<ImageEvent>;
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum ImageEvent {
    Start {
        warnings: Vec<Warning>,
    },
    PartialImage {
        index: u32,
        image: ContentBlock<Image>,
    },
    Finish {
        result: ImageResult,
    },
    Error {
        error: Error,
    },
    #[serde(untagged)]
    Unknown(serde_json::Value),
}
impl MediaStream<ImageEvent> {
    pub async fn aggregate(mut self) -> Result<ImageResult> {
        while let Some(event) = self.next().await {
            match event {
                ImageEvent::Finish { result } => return Ok(result),
                ImageEvent::Error { error } => return Err(error),
                _ => {}
            }
        }
        Err(Error::invalid("image stream has no finish event"))
    }
}
impl<B: Backend> Client<B> {
    pub async fn stream_images(&self, request: &ImageRequest) -> Result<ImageStream> {
        self.image_stream(request, Vec::new()).await
    }
    pub async fn stream_images_prepared(
        &self,
        prepared: PreparedImageRequest,
    ) -> Result<ImageStream> {
        self.image_stream(&prepared.request, prepared.warnings)
            .await
    }
    async fn image_stream(
        &self,
        request: &ImageRequest,
        mut warnings: Vec<Warning>,
    ) -> Result<ImageStream> {
        request.target()?;
        if request.model.dialect == ImageDialect::Gemini {
            shared::compatibility(
                &mut warnings,
                "stream",
                "Gemini image generation emits a single completion rather than progressive images",
            );
            let mut result = self.generate_images(request).await?;
            warnings.extend(result.warnings);
            result.warnings = warnings.clone();
            return Ok(MediaStream::ready(vec![
                ImageEvent::Start { warnings },
                ImageEvent::Finish { result },
            ]));
        }
        let rendered = openai::render(request, true)?;
        warnings.extend(rendered.warnings);
        let editing = !request.images.is_empty();
        let (operation, wire) = rendered.request.wire()?;
        let response = self
            .send(
                OperationKey {
                    operation,
                    dialect: protocol::Dialect::OpenAi,
                },
                Some(&request.model.id),
                wire,
            )
            .await?;
        if io::is_json(&response.headers) {
            shared::compatibility(
                &mut warnings,
                "stream",
                "The upstream returned completed images instead of progressive SSE",
            );
            let format = openai::output_format(request.output_format.as_ref(), &mut Vec::new());
            let result = io::read(response, self.limits, |body, head| {
                let mut result = openai::parse(
                    io::parse(std::str::from_utf8(body).map_err(Error::invalid)?)?,
                    format,
                )?;
                result.response.merge(head.metadata());
                let mut all = warnings.clone();
                all.extend(result.warnings);
                result.warnings = all;
                Ok(result)
            })
            .await?;
            return Ok(MediaStream::ready(vec![
                ImageEvent::Start { warnings },
                ImageEvent::Finish { result },
            ]));
        }
        let mut state = State {
            editing,
            result: ImageResult {
                images: Vec::new(),
                text: Vec::new(),
                usage: OperationUsage::default(),
                warnings: warnings.clone(),
                finish_reason: io::interrupted(),
                response: ResponseMetadata::default(),
                provider_metadata: ProviderMetadata::default(),
            },
            complete: false,
            required: request.n,
        };
        Ok(io::sse(
            response,
            self.limits,
            vec![ImageEvent::Start { warnings }],
            Box::new(move |input, head| state.step(input, head)),
        ))
    }
}
struct State {
    editing: bool,
    result: ImageResult,
    complete: bool,
    required: std::num::NonZeroU32,
}
impl State {
    fn step(&mut self, input: Input, head: &Head) -> (Vec<ImageEvent>, bool) {
        match input {
            Input::Data(data) => match self.push(&data, head) {
                Ok(mut events) => {
                    let done =
                        self.complete && self.result.images.len() >= self.required.get() as usize;
                    if done {
                        events.push(ImageEvent::Finish {
                            result: self.finish(io::stop()),
                        });
                    }
                    (events, done)
                }
                Err(e) => self.fail(head.error(e.with_body(data.as_bytes()))),
            },
            Input::Fault(error) => self.fail(error),
            Input::End => {
                if self.complete && self.result.images.len() < self.required.get() as usize {
                    shared::compatibility(
                        &mut self.result.warnings,
                        "n",
                        "The upstream completed with fewer images than requested",
                    );
                }
                let reason = if self.complete {
                    io::stop()
                } else {
                    io::interrupted()
                };
                (
                    vec![ImageEvent::Finish {
                        result: self.finish(reason),
                    }],
                    true,
                )
            }
        }
    }
    fn finish(&mut self, reason: FinishReason) -> ImageResult {
        ImageResult {
            images: std::mem::take(&mut self.result.images),
            text: std::mem::take(&mut self.result.text),
            finish_reason: reason,
            usage: std::mem::take(&mut self.result.usage),
            warnings: std::mem::take(&mut self.result.warnings),
            response: std::mem::take(&mut self.result.response),
            provider_metadata: std::mem::take(&mut self.result.provider_metadata),
        }
    }
    fn fail(&mut self, error: Error) -> (Vec<ImageEvent>, bool) {
        let reason = if matches!(
            error.kind,
            ErrorKind::InvalidData | ErrorKind::Network | ErrorKind::Timeout
        ) {
            io::interrupted()
        } else {
            FinishReason::new(FinishReasonKind::Error, None)
        };
        (
            vec![
                ImageEvent::Error { error },
                ImageEvent::Finish {
                    result: self.finish(reason),
                },
            ],
            true,
        )
    }
    fn push(&mut self, data: &str, head: &Head) -> Result<Vec<ImageEvent>> {
        enum Event {
            Partial(o::ImagePartialEvent),
            Done(o::ImageCompletedEvent),
        }
        let event = if self.editing {
            match io::parse::<o::ImageEditStreamEvent>(data)? {
                o::ImageEditStreamEvent::Partial(p) => Event::Partial(p),
                o::ImageEditStreamEvent::Completed(p) => Event::Done(p),
            }
        } else {
            match io::parse::<o::ImageGenerationStreamEvent>(data)? {
                o::ImageGenerationStreamEvent::Partial(p) => Event::Partial(p),
                o::ImageGenerationStreamEvent::Completed(p) => Event::Done(p),
            }
        };
        #[derive(Serialize)]
        struct Extra {
            background: o::ImageBackground,
            quality: o::StreamImageQuality,
            size: String,
            #[serde(flatten)]
            rest: protocol::Rest,
        }
        let (data, format, extra, index, created) = match event {
            Event::Partial(p) => (
                p.b64_json,
                p.output_format,
                Extra {
                    background: p.background,
                    quality: p.quality,
                    size: p.size,
                    rest: p.rest,
                },
                Some(p.partial_image_index),
                p.created_at,
            ),
            Event::Done(p) => {
                self.complete = true;
                self.result.usage = openai::usage(Some(&p.usage))?;
                (
                    p.b64_json,
                    p.output_format,
                    Extra {
                        background: p.background,
                        quality: p.quality,
                        size: p.size,
                        rest: p.rest,
                    },
                    None,
                    p.created_at,
                )
            }
        };
        self.result.response.merge(head.metadata());
        self.result.response.timestamp = Some(created);
        let image = ContentBlock {
            content: Image {
                source: Asset::Inline {
                    bytes: base64::engine::general_purpose::STANDARD
                        .decode(data)
                        .map_err(Error::invalid)?,
                },
                media_type: Some(openai::mime(format).into()),
                detail: Detail::Auto,
            },
            cache: None,
            provider_metadata: [(Vendor::Openai, shared::metadata(extra)?)].into(),
        };
        if let Some(index) = index {
            Ok(vec![ImageEvent::PartialImage {
                index: index.try_into().map_err(Error::invalid)?,
                image,
            }])
        } else {
            self.result.images.push(image);
            Ok(Vec::new())
        }
    }
}
