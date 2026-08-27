use serde::Deserialize;

#[derive(Deserialize)]
struct Release {
    body: Option<String>,
}

pub(super) async fn fetch(client: &wreq::Client, version: &str) -> Option<String> {
    let tag = if version.starts_with('v') {
        version.to_owned()
    } else {
        format!("v{version}")
    };
    let url = format!("https://api.github.com/repos/LeenHawk/gproxy/releases/tags/{tag}");
    let response = client.get(url).send().await.ok()?;
    if !response.status().is_success() {
        return None;
    }
    let bytes = response.bytes().await.ok()?;
    serde_json::from_slice::<Release>(&bytes).ok()?.body
}
