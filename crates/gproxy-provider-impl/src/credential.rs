use serde_json::Value;

#[derive(Debug, Clone)]
pub struct BaseCredential {
    pub id: i64,
    pub name: Option<String>,
    pub secret: Value,
    pub meta: Value,
}
