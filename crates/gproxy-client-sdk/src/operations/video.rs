use super::empty_request;
use crate::*;
use gproxy_protocol::{Operation, openai};
use http::Method;
#[cfg(feature = "video")]
impl<B: Backend> Client<B> {
    pub async fn retrieve_video(&self, id: &str) -> Result<openai::video::NativeVideo> {
        self.json(
            OperationKey {
                operation: Operation::RetrieveVideo,
                dialect: gproxy_protocol::Dialect::OpenAi,
            },
            None,
            empty_request(Method::GET, format!("/v1/videos/{}", path_segment(id))),
        )
        .await
    }
    pub async fn list_videos(
        &self,
        query: &openai::video::NativeListVideosQuery,
    ) -> Result<openai::video::NativeListVideosResponseBody> {
        let mut request = empty_request(Method::GET, "/v1/videos".into());
        request.query = Some(serde_urlencoded::to_string(query).map_err(Error::invalid)?);
        self.json(
            OperationKey {
                operation: Operation::ListVideos,
                dialect: gproxy_protocol::Dialect::OpenAi,
            },
            None,
            request,
        )
        .await
    }
    pub async fn delete_video(
        &self,
        id: &str,
    ) -> Result<openai::video::NativeDeleteVideoResponseBody> {
        self.json(
            OperationKey {
                operation: Operation::DeleteVideo,
                dialect: gproxy_protocol::Dialect::OpenAi,
            },
            None,
            empty_request(Method::DELETE, format!("/v1/videos/{}", path_segment(id))),
        )
        .await
    }
    pub async fn download_video(&self, id: &str) -> Result<WireResponse<HttpBody>> {
        self.send(
            OperationKey {
                operation: Operation::DownloadVideoContent,
                dialect: gproxy_protocol::Dialect::OpenAi,
            },
            None,
            empty_request(
                Method::GET,
                format!("/v1/videos/{}/content", path_segment(id)),
            ),
        )
        .await
    }
}
