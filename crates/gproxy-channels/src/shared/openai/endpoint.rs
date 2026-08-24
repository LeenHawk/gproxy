use gproxy_protocol::Operation;

pub(crate) fn replace_resource(mut url: String, operation: Operation, path: &str) -> String {
    let replacement = match operation {
        Operation::RetrieveFile | Operation::RetrieveFileContent | Operation::DeleteFile => {
            file_id(path).map(|id| ("{file_id}", id))
        }
        Operation::RetrieveVideo
        | Operation::DeleteVideo
        | Operation::DownloadVideoContent
        | Operation::RemixVideo => video_id(path).map(|id| ("{video_id}", id)),
        Operation::GetVideoCharacter => character_id(path).map(|id| ("{character_id}", id)),
        _ => None,
    };
    if let Some((slot, value)) = replacement {
        url = url.replace(slot, &crate::shared::http::encode_component(value));
    }
    url
}

fn file_id(path: &str) -> Option<&str> {
    path.strip_prefix("/v1/files/")?
        .strip_suffix("/content")
        .or_else(|| path.strip_prefix("/v1/files/"))
        .filter(|id| !id.is_empty() && !id.contains('/'))
}

fn video_id(path: &str) -> Option<&str> {
    path.strip_prefix("/v1/videos/")?
        .split('/')
        .next()
        .filter(|id| !id.is_empty())
}

fn character_id(path: &str) -> Option<&str> {
    path.strip_prefix("/v1/videos/characters/")
        .filter(|id| !id.is_empty() && !id.contains('/'))
}
