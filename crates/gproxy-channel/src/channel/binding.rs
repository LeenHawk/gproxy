//! Direct operation dispatch with caller-selected configuration and client.

use gproxy_protocol::{
    HttpBody, Operation, OperationKey, WireRequest, WireResponse, capability::UpstreamConnection,
};

use super::{BaseChannel, ChannelError, CredentialView, OperationContext, ProviderView};
use gproxy_client::OutboundClient;

/// One explicit channel + provider + credential + client binding.
/// An operation can perform multiple exchanges, all through its assigned client.
/// The future engine supplies a client wrapper for accounting and capture of each
/// exchange; this binding does not implement routing, settlement or persistence.
pub struct ChannelBinding<'a> {
    channel: &'a dyn BaseChannel,
    provider: ProviderView<'a>,
    credential: CredentialView<'a>,
    client: &'a dyn OutboundClient,
    endpoint_override: Option<&'a str>,
}

impl<'a> ChannelBinding<'a> {
    pub fn new(
        channel: &'a dyn BaseChannel,
        provider: ProviderView<'a>,
        credential: CredentialView<'a>,
        client: &'a dyn OutboundClient,
    ) -> Self {
        Self {
            channel,
            provider,
            credential,
            client,
            endpoint_override: None,
        }
    }

    /// Use a configured complete method URL for the dispatched operation.
    pub fn endpoint(mut self, url: Option<&'a str>) -> Self {
        self.endpoint_override = url;
        self
    }

    /// Dispatch an HTTP operation to its named method, preserving streaming bodies.
    pub async fn send(
        &self,
        operation: OperationKey,
        request: WireRequest<HttpBody>,
    ) -> Result<WireResponse<HttpBody>, ChannelError> {
        let context = OperationContext {
            provider: self.provider,
            credential: self.credential,
            dialect: operation.dialect,
            request,
            client: self.client,
            endpoint_override: self.endpoint_override,
        };
        match operation.operation {
            Operation::ListModels => self.channel.list_models(context).await,
            Operation::GetModel => self.channel.get_model(context).await,
            Operation::CountTokens => self.channel.count_tokens(context).await,
            Operation::GenerateContent => self.channel.generate_content(context).await,
            Operation::StreamGenerateContent => self.channel.stream_generate_content(context).await,
            Operation::GuardianReview => self.channel.guardian_review(context).await,
            Operation::GuardianClassify => self.channel.guardian_classify(context).await,
            Operation::CompactContent => self.channel.compact_content(context).await,
            Operation::SummarizeMemory => self.channel.summarize_memory(context).await,
            Operation::CreateConversation => self.channel.create_conversation(context).await,
            Operation::CreateEmbedding => self.channel.create_embedding(context).await,
            Operation::BatchCreateEmbedding => self.channel.batch_create_embedding(context).await,
            Operation::Rerank => self.channel.rerank(context).await,
            Operation::WebSearch => self.channel.web_search(context).await,
            Operation::CreateImage => self.channel.create_image(context).await,
            Operation::EditImage => self.channel.edit_image(context).await,
            Operation::CreateSpeech => self.channel.create_speech(context).await,
            Operation::CreateTranscription => self.channel.create_transcription(context).await,
            Operation::CreateTranslation => self.channel.create_translation(context).await,
            Operation::CreateFile => self.channel.create_file(context).await,
            Operation::ListFiles => self.channel.list_files(context).await,
            Operation::RetrieveFile => self.channel.retrieve_file(context).await,
            Operation::RetrieveFileContent => self.channel.retrieve_file_content(context).await,
            Operation::DeleteFile => self.channel.delete_file(context).await,
            Operation::CreateVideo => self.channel.create_video(context).await,
            Operation::RetrieveVideo => self.channel.retrieve_video(context).await,
            Operation::ListVideos => self.channel.list_videos(context).await,
            Operation::DeleteVideo => self.channel.delete_video(context).await,
            Operation::DownloadVideoContent => self.channel.download_video_content(context).await,
            Operation::CreateRealtimeCall => self.channel.create_realtime_call(context).await,
            Operation::ConnectRealtime => Err(ChannelError::WrongTransport(operation)),
        }
    }

    /// Dispatch a WebSocket operation. A rejected upgrade retains its HTTP body;
    /// a successful upgrade returns the independent incoming/outgoing connection.
    pub async fn connect(
        &self,
        operation: OperationKey,
        request: WireRequest<()>,
    ) -> Result<UpstreamConnection, ChannelError> {
        let context = OperationContext {
            provider: self.provider,
            credential: self.credential,
            dialect: operation.dialect,
            request,
            client: self.client,
            endpoint_override: self.endpoint_override,
        };
        match operation.operation {
            Operation::ConnectRealtime => self.channel.connect_realtime(context).await,
            Operation::GenerateContent => self.channel.generate_content_websocket(context).await,
            Operation::StreamGenerateContent => {
                self.channel
                    .stream_generate_content_websocket(context)
                    .await
            }
            _ => Err(ChannelError::WrongTransport(operation)),
        }
    }
}
