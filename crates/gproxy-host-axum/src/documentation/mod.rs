//! Build-only OpenAPI export from typed Rust payloads and host route metadata.
mod protocol;
mod schemas;

use gproxy_protocol::{OperationKey, spec::OPERATION_SPECS};
use schemars::JsonSchema;
pub(crate) use schemas::Schemas;
use schemas::alternatives;
use serde_json::{Map, Value, json};

pub(crate) struct Api {
    pub document: Value,
    pub schemas: Schemas,
}

pub fn openapi() -> Value {
    let mut api = Api {
        document: serde_json::to_value(crate::ingress::surface::openapi()).unwrap(),
        schemas: Schemas::default(),
    };
    api.document["info"]["title"] = json!("GPROXY HTTP API");
    api.document["info"]["description"] = json!(
        "Forwarding, management, portal and OAuth APIs generated from Rust types. Input and output schemas preserve their respective Serde contracts. Streaming events and WebSocket messages are documented alongside HTTP payloads."
    );
    api.document["components"]["securitySchemes"]["consoleSession"] = json!({
        "type":"apiKey", "in":"cookie", "name":crate::session::COOKIE_NAME,
    });

    // A protocol declaration without a documented payload is a build failure,
    // even if it is reachable only through streaming rather than a new URL.
    for spec in OPERATION_SPECS {
        protocol::payloads(&mut api.schemas, spec.key, "");
    }
    for (path, methods) in api.document["paths"].as_object_mut().unwrap() {
        for operation in methods.as_object_mut().unwrap().values_mut() {
            let mut request = Map::new();
            let mut response = Map::new();
            let mut websocket = None;
            let mut parameters = operation["parameters"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            for route in operation["x-gproxy-routes"].as_array_mut().unwrap() {
                let key: OperationKey = serde_json::from_value(json!({
                    "operation":route["operation"], "dialect":route["dialect"],
                }))
                .unwrap();
                let shape = protocol::payloads(&mut api.schemas, key, path);
                route["requestContent"] = json!(shape.request);
                route["responseContent"] = json!(shape.response);
                if let Some(messages) = shape.websocket {
                    route["websocketMessages"] = messages.clone();
                    websocket = Some(messages);
                }
                merge_content(&mut request, shape.request);
                merge_content(&mut response, shape.response);
                merge_parameters(&mut parameters, shape.parameters);
                if !route["streaming"].is_null() {
                    let streaming: OperationKey = serde_json::from_value(json!({
                        "operation": route["streaming"], "dialect":route["dialect"],
                    }))
                    .unwrap();
                    let stream = protocol::payloads(&mut api.schemas, streaming, path);
                    route["streamingResponseContent"] = json!(stream.response);
                    merge_content(&mut response, stream.response);
                }
            }
            // WebSocket messages belong to the upgrade, never an HTTP body.
            if let Some(messages) = websocket {
                operation["x-websocket"] = messages;
            }
            if request.is_empty() {
                operation.as_object_mut().unwrap().remove("requestBody");
            } else {
                operation["requestBody"] = json!({"required":true, "content":request});
            }
            if !response.is_empty() {
                operation["responses"]["2XX"] =
                    json!({"description":"Successful protocol response", "content":response});
            }
            operation["parameters"] = json!(parameters);
            operation["responses"]["default"] = protocol::error_response(&mut api.schemas);
        }
    }
    crate::admin::document(&mut api);
    crate::portal::document(&mut api);
    crate::oauth::document(&mut api);
    crate::update::document(&mut api);
    crate::document_system(&mut api);
    api.document["components"]["schemas"] = json!(api.schemas.definitions());
    let expected: std::collections::BTreeSet<_> =
        include_str!(concat!(env!("OUT_DIR"), "/documented-routes.txt"))
            .lines()
            .map(str::to_owned)
            .collect();
    let actual: std::collections::BTreeSet<_> = api.document["paths"]
        .as_object()
        .unwrap()
        .iter()
        .filter(|(path, _)| path.starts_with("/admin/api/") || path.starts_with("/portal/api/"))
        .flat_map(|(path, methods)| {
            methods
                .as_object()
                .unwrap()
                .keys()
                .map(move |method| format!("{method} {path}"))
        })
        .collect();
    assert_eq!(
        expected, actual,
        "management/portal documentation must exactly cover the router declarations"
    );
    api.document
}

fn merge_content(target: &mut Map<String, Value>, incoming: Map<String, Value>) {
    for (media, content) in incoming {
        if let Some(existing) = target.get_mut(&media) {
            for key in ["schema", "x-event-schema"] {
                if let Some(schema) = content.get(key) {
                    existing[key] = if let Some(previous) = existing.get(key) {
                        alternatives(vec![previous.clone(), schema.clone()])
                    } else {
                        schema.clone()
                    };
                }
            }
        } else {
            target.insert(media, content);
        }
    }
}

fn merge_parameters(target: &mut Vec<Value>, incoming: Vec<Value>) {
    for parameter in incoming {
        if let Some(existing) = target
            .iter_mut()
            .find(|old| old["in"] == parameter["in"] && old["name"] == parameter["name"])
        {
            existing["schema"] = alternatives(vec![
                existing["schema"].clone(),
                parameter["schema"].clone(),
            ]);
        } else {
            target.push(parameter);
        }
    }
}

pub(crate) fn error_response(schemas: &mut Schemas) -> Value {
    json!({"description":"Gateway error", "content":{"application/json":{"schema":crate::error::schema(schemas)}}})
}

impl Api {
    pub fn route(
        &mut self,
        method: &str,
        path: &str,
        section: &str,
        request: Option<Value>,
        mut parameters: Vec<Value>,
        response: Option<Value>,
    ) -> &mut Value {
        for name in path
            .split('{')
            .skip(1)
            .map(|part| part.split('}').next().unwrap())
        {
            parameters
                .push(json!({"in":"path","name":name,"required":true,"schema":{"type":"string"}}));
        }
        if path.starts_with("/admin/api/") && !section.is_empty() {
            parameters.push(json!({"in":"header","name":gproxy_app::SCOPE_HEADER,"required":false,"schema":{"type":"string"},"description":"Administrative scope selected by the caller."}));
        }
        let tag = if path.starts_with("/admin/") {
            "Management"
        } else if path.starts_with("/portal/") {
            "Portal"
        } else if path.contains("oauth") {
            "OAuth"
        } else {
            "System"
        };
        let mut operation = json!({
            "tags":[tag], "summary":format!("{} {path}",method.to_uppercase()),
            "parameters":parameters, "x-gproxy-section":section,
            "security":[{"consoleSession":[]},{"bearer":[]},{"anthropicKey":[]},{"googleKey":[]}],
            "responses":{"default":error_response(&mut self.schemas)},
        });
        if let Some(body) = request {
            operation["requestBody"] =
                json!({"required":true,"content":{"application/json":{"schema":body}}});
        }
        if let Some(body) = response {
            operation["responses"]["200"] =
                json!({"description":"Success","content":{"application/json":{"schema":body}}});
        } else {
            operation["responses"]["204"] =
                json!({"description":"Success without a response body"});
        }
        let methods = self.document["paths"]
            .as_object_mut()
            .unwrap()
            .entry(path)
            .or_insert_with(|| json!({}));
        assert!(
            methods.get(method).is_none(),
            "duplicate API operation: {method} {path}"
        );
        methods[method] = operation;
        &mut methods[method]
    }

    pub fn get<R: JsonSchema>(&mut self, path: &str, section: &str) -> &mut Value {
        let response = self.schemas.output::<R>();
        self.route("get", path, section, None, vec![], Some(response))
    }
    pub fn query<Q: JsonSchema, R: JsonSchema>(&mut self, path: &str, section: &str) -> &mut Value {
        let query = self.schemas.query::<Q>();
        let response = self.schemas.output::<R>();
        self.route("get", path, section, None, query, Some(response))
    }
    pub fn json<W: JsonSchema, R: JsonSchema>(
        &mut self,
        method: &str,
        path: &str,
        section: &str,
    ) -> &mut Value {
        let request = self.schemas.input::<W>();
        let response = self.schemas.output::<R>();
        self.route(method, path, section, Some(request), vec![], Some(response))
    }
    pub fn action<R: JsonSchema>(&mut self, path: &str, section: &str) -> &mut Value {
        let response = self.schemas.output::<R>();
        self.route("post", path, section, None, vec![], Some(response))
    }
    pub fn write_empty<W: JsonSchema>(
        &mut self,
        method: &str,
        path: &str,
        section: &str,
    ) -> &mut Value {
        let request = self.schemas.input::<W>();
        self.route(method, path, section, Some(request), vec![], None)
    }
    pub fn empty(&mut self, method: &str, path: &str, section: &str) -> &mut Value {
        self.route(method, path, section, None, vec![], None)
    }
}
