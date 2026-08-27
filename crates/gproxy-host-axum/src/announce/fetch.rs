use super::model::Feed;

const FEED_URL: &str = "https://gproxy.leenhawk.com/notifications.json";
const SIGNATURE_URL: &str = "https://gproxy.leenhawk.com/notifications.json.sig";

pub(super) async fn fetch(client: &wreq::Client) -> Option<(Vec<u8>, String)> {
    let bytes = get(client, FEED_URL).await?;
    let signature = get(client, SIGNATURE_URL).await?;
    let signature = String::from_utf8(signature).ok()?;
    Some((bytes, signature))
}

pub(super) fn verified(bytes: &[u8], signature: &str) -> Option<Feed> {
    crate::signature::verify_detached(bytes, signature).ok()?;
    let feed = serde_json::from_slice::<Feed>(bytes).ok()?;
    (feed.version == 1).then_some(feed)
}

#[cfg(test)]
pub(super) fn verified_with_key(bytes: &[u8], signature: &str, public_key: &str) -> Option<Feed> {
    crate::signature::verify_detached_with_key(bytes, signature, Some(public_key)).ok()?;
    let feed = serde_json::from_slice::<Feed>(bytes).ok()?;
    (feed.version == 1).then_some(feed)
}

async fn get(client: &wreq::Client, url: &str) -> Option<Vec<u8>> {
    let response = client.get(url).send().await.ok()?;
    if !response.status().is_success() {
        return None;
    }
    response.bytes().await.ok().map(|bytes| bytes.to_vec())
}
