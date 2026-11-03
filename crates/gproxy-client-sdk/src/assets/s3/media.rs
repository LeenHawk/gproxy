use super::*;
#[cfg(feature = "images")]
impl crate::images::ImageRequest {
    pub async fn resolve_assets_s3<F, Fut>(
        &self,
        store: &S3AssetResolver,
        mut load: F,
    ) -> Result<crate::images::PreparedImageRequest>
    where
        F: FnMut(String) -> Fut,
        Fut: std::future::Future<Output = Result<AssetData>>,
    {
        self.resolve_assets(DeliveryMode::PreferUrl, |input| {
            let data = load(input.reference.clone());
            async move { store.resolve(input, data.await?).await }
        })
        .await
    }
}
#[cfg(feature = "transcription")]
impl crate::transcription::TranscriptionRequest {
    pub async fn resolve_assets_s3<F, Fut>(
        &self,
        store: &S3AssetResolver,
        mut load: F,
    ) -> Result<crate::transcription::PreparedTranscriptionRequest>
    where
        F: FnMut(String) -> Fut,
        Fut: std::future::Future<Output = Result<AssetData>>,
    {
        self.resolve_assets(DeliveryMode::PreferUrl, |input| {
            let data = load(input.reference.clone());
            async move { store.resolve(input, data.await?).await }
        })
        .await
    }
}
impl<B: Backend> S3Client<B> {
    #[cfg(feature = "images")]
    pub async fn generate_images<F, Fut>(
        &self,
        request: &crate::images::ImageRequest,
        load: F,
    ) -> Result<crate::images::ImageResult>
    where
        F: FnMut(String) -> Fut,
        Fut: std::future::Future<Output = Result<AssetData>>,
    {
        self.client
            .generate_images_prepared(request.resolve_assets_s3(&self.assets, load).await?)
            .await
    }
    #[cfg(feature = "images")]
    pub async fn stream_images<F, Fut>(
        &self,
        request: &crate::images::ImageRequest,
        load: F,
    ) -> Result<crate::images::ImageStream>
    where
        F: FnMut(String) -> Fut,
        Fut: std::future::Future<Output = Result<AssetData>>,
    {
        self.client
            .stream_images_prepared(request.resolve_assets_s3(&self.assets, load).await?)
            .await
    }
    #[cfg(feature = "speech")]
    pub async fn synthesize(
        &self,
        request: &crate::speech::SpeechRequest,
    ) -> Result<crate::speech::SpeechResult> {
        self.client.synthesize(request).await
    }
    #[cfg(feature = "speech")]
    pub async fn stream_speech(
        &self,
        request: &crate::speech::SpeechRequest,
    ) -> Result<crate::speech::SpeechStream> {
        self.client.stream_speech(request).await
    }
    #[cfg(feature = "transcription")]
    pub async fn transcribe_audio<F, Fut>(
        &self,
        request: &crate::transcription::TranscriptionRequest,
        load: F,
    ) -> Result<crate::transcription::TranscriptionResult>
    where
        F: FnMut(String) -> Fut,
        Fut: std::future::Future<Output = Result<AssetData>>,
    {
        self.client
            .transcribe_audio_prepared(request.resolve_assets_s3(&self.assets, load).await?)
            .await
    }
    #[cfg(feature = "transcription")]
    pub async fn stream_transcription<F, Fut>(
        &self,
        request: &crate::transcription::TranscriptionRequest,
        load: F,
    ) -> Result<crate::transcription::TranscriptionStream>
    where
        F: FnMut(String) -> Fut,
        Fut: std::future::Future<Output = Result<AssetData>>,
    {
        self.client
            .stream_transcription_prepared(request.resolve_assets_s3(&self.assets, load).await?)
            .await
    }
}
