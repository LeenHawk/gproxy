use super::{auth, config::invalid};
use crate::channel::{ChannelError, OperationContext, PrepareContext};
use crate::channels::shared::compatible::http::read_body;
use gproxy_protocol::{HttpBody, Operation, OperationKey, WireResponse};
use http::{HeaderValue, Method, header};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

const URL: &str = "https://dnspod.cloud.tencent.com/ajax/rainbow?action=getRainbowConfig";
const GROUP: &str = "orcaterm.内置模型";

pub(super) fn prepare(ctx: PrepareContext<'_>) -> Result<http::Request<HttpBody>, ChannelError> {
    // Public configuration, independently verified without cookies. Never send
    // an account credential or caller headers to this separate DNSPod origin.
    http::Request::builder()
        .method(Method::POST)
        .uri(ctx.endpoint_override.unwrap_or(URL))
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::ACCEPT, "application/json")
        .body(HttpBody::Bytes(
            json!({"app":"ai","groups":[GROUP],"options":{"enableEnv":1}})
                .to_string()
                .into(),
        ))
        .map_err(|e| ChannelError::InvalidConfig(e.to_string()))
}

fn number(value: &Value) -> f64 {
    value
        .as_f64()
        .or_else(|| value.as_str()?.parse().ok())
        .unwrap_or(0.0)
}

fn catalog(root: &Value, user_id: &str) -> Result<Value, ChannelError> {
    if root["code"] != 0 {
        return Err(invalid("model configuration request failed"));
    }
    let rows = root["data"]
        .as_array()
        .and_then(|groups| groups.iter().find(|g| g["group"] == GROUP))
        .and_then(|group| group["rows"].as_array())
        .ok_or_else(|| invalid("model configuration has no group rows"))?;
    let mut sorted = rows
        .iter()
        .filter(|row| number(&row["weight"]) > 0.0)
        .collect::<Vec<_>>();
    sorted.sort_by(|a, b| number(&a["sort"]).total_cmp(&number(&b["sort"])));
    let mut groups: BTreeMap<&str, Vec<&Value>> = BTreeMap::new();
    for row in sorted {
        groups
            .entry(row["text"].as_str().unwrap_or("default"))
            .or_default()
            .push(row);
    }
    // The client selects a weighted variant per display group using the last
    // two decimal digits of the account id. Concrete model IDs stay verbatim.
    let suffix = user_id
        .chars()
        .rev()
        .take(2)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<String>();
    let bucket = if suffix.is_empty() {
        0.0
    } else {
        // JavaScript parseInt accepts a decimal prefix; NaN deliberately
        // selects the final variant, matching the desktop's fallback.
        suffix
            .chars()
            .take_while(char::is_ascii_digit)
            .collect::<String>()
            .parse::<f64>()
            .unwrap_or(f64::NAN)
            % 100.0
    };
    let mut chosen = Vec::new();
    for rows in groups.values() {
        let threshold = bucket / 100.0 * rows.iter().map(|r| number(&r["weight"])).sum::<f64>();
        let mut sum = 0.0;
        let row = rows
            .iter()
            .find(|r| {
                sum += number(&r["weight"]);
                threshold < sum
            })
            .copied()
            .or_else(|| rows.last().copied())
            .expect("nonempty group");
        chosen.push(row);
    }
    chosen.sort_by(|a, b| number(&a["sort"]).total_cmp(&number(&b["sort"])));
    let mut ids = BTreeSet::new();
    let mut data = Vec::new();
    for row in chosen {
        let id = row["model"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| invalid("enabled model has no id"))?;
        if !ids.insert(id) {
            continue;
        }
        data.push(json!({"id":id,"object":"model","created":0,"owned_by":"orcaterm",
            "name":row.get("displayName").filter(|v| v.as_str().is_some_and(|s| !s.is_empty())).unwrap_or(&row["text"]),
            "description":row["description"],"input_modalities":["text"],"output_modalities":["text"]}));
    }
    Ok(json!({"object":"list","data":data}))
}

pub(super) async fn list(
    ctx: OperationContext<'_>,
) -> Result<WireResponse<HttpBody>, ChannelError> {
    let request = super::request::prepare(PrepareContext {
        provider: ctx.provider,
        credential: ctx.credential,
        operation: OperationKey {
            operation: Operation::ListModels,
            dialect: ctx.dialect,
        },
        request: ctx.request,
        endpoint_override: ctx.endpoint_override,
    })?;
    let mut response = ctx.client.send(request).await?;
    if !response.status.is_success() {
        return Ok(response);
    }
    let bytes = read_body(response.body).await?;
    let root = serde_json::from_slice(&bytes).map_err(|e| invalid(e.to_string()))?;
    response.body = HttpBody::Bytes(
        catalog(
            &root,
            auth::fact(ctx.credential, "user_id").unwrap_or_default(),
        )?
        .to_string()
        .into(),
    );
    response.headers.remove(header::CONTENT_LENGTH);
    response.headers.remove(header::CONTENT_ENCODING);
    response.headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    Ok(response)
}
