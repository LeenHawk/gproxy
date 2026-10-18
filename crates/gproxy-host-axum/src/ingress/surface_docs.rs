//! OpenAPI projection of the tables used by the ingress matcher.
//!
//! A shared method/path can select several dialects. Keep those rows together
//! in one OpenAPI operation rather than letting the last dialect overwrite it.

use std::collections::BTreeMap;

use serde_json::json;
use utoipa::openapi::{
    ComponentsBuilder, InfoBuilder, OpenApi, OpenApiBuilder, Required,
    path::{HttpMethod, OperationBuilder, ParameterBuilder, ParameterIn, PathItem, PathsBuilder},
    response::ResponseBuilder,
    schema::{ObjectBuilder, Type},
    security::{ApiKey, ApiKeyValue, Http, HttpAuthScheme, SecurityRequirement, SecurityScheme},
};

use super::{Surface, codex_table, table};

/// Forwarding paths. The complete exporter adds typed payloads and the
/// management, portal and OAuth surfaces before publishing the document.
pub fn openapi() -> OpenApi {
    let mut groups: BTreeMap<(String, &str), Vec<&Surface>> = BTreeMap::new();
    for (prefix, surfaces) in [("", table()), ("/{provider}", codex_table())] {
        for surface in surfaces {
            groups
                .entry((
                    format!("{prefix}{}", surface.pattern),
                    surface.method.as_str(),
                ))
                .or_default()
                .push(surface);
        }
    }

    let mut paths = PathsBuilder::new();
    for ((path, method), surfaces) in groups {
        let codex_only = path.starts_with("/{provider}/");
        let routes: Vec<_> = surfaces
            .iter()
            .map(|surface| {
                json!({
                    "dialect": surface.dialect,
                    "operation": surface.operation,
                    "streaming": surface.streaming,
                    "websocket": surface.upgrade,
                    "modelParameter": surface.model_param,
                })
            })
            .collect();
        let mut operation = OperationBuilder::new()
            .tag(if codex_only { "Codex provider" } else { "Forwarding" })
            .summary(Some(format!("{method} {path}")))
            .description(Some(if codex_only {
                "Only available on a Codex provider mount. The provider parameter is the configured provider name."
            } else {
                "Available at the instance root or under a provider/namespace prefix. Shared paths select the wire dialect from client headers; without dialect evidence, the first listed route wins."
            }))
            .extensions(Some([("x-gproxy-routes", json!(routes))].into_iter().collect()));
        for parameter in path
            .split('{')
            .skip(1)
            .filter_map(|part| part.split('}').next())
        {
            operation = operation.parameter(
                ParameterBuilder::new()
                    .name(parameter)
                    .parameter_in(ParameterIn::Path)
                    .required(Required::True)
                    .schema(Some(ObjectBuilder::new().schema_type(Type::String))),
            );
        }
        if surfaces.iter().any(|surface| surface.upgrade) {
            operation = operation.response(
                "101",
                ResponseBuilder::new().description("WebSocket upgrade"),
            );
        }
        let method = match method {
            "GET" => HttpMethod::Get,
            "POST" => HttpMethod::Post,
            "DELETE" => HttpMethod::Delete,
            "PUT" => HttpMethod::Put,
            "PATCH" => HttpMethod::Patch,
            "HEAD" => HttpMethod::Head,
            "OPTIONS" => HttpMethod::Options,
            other => panic!("undocumented ingress method: {other}"),
        };
        paths = paths.path(path, PathItem::new(method, operation.build()));
    }

    OpenApiBuilder::new()
        .info(
            InfoBuilder::new()
                .title("GPROXY forwarding API")
                .version(env!("CARGO_PKG_VERSION"))
                .build(),
        )
        .paths(paths.build())
        .components(Some(
            ComponentsBuilder::new()
                .security_scheme(
                    "bearer",
                    SecurityScheme::Http(Http::new(HttpAuthScheme::Bearer)),
                )
                .security_scheme(
                    "anthropicKey",
                    SecurityScheme::ApiKey(ApiKey::Header(ApiKeyValue::new("x-api-key"))),
                )
                .security_scheme(
                    "googleKey",
                    SecurityScheme::ApiKey(ApiKey::Header(ApiKeyValue::new("x-goog-api-key"))),
                )
                .security_scheme(
                    "queryKey",
                    SecurityScheme::ApiKey(ApiKey::Query(ApiKeyValue::new("key"))),
                )
                .build(),
        ))
        .security(Some(
            ["bearer", "anthropicKey", "googleKey", "queryKey"]
                .map(|name| SecurityRequirement::new(name, Vec::<String>::new())),
        ))
        .build()
}
