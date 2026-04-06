use serde::{Deserialize, Serialize};

use super::Scope;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct UserQuery {
    pub id: Scope<i64>,
    pub name: Scope<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserQueryRow {
    pub id: i64,
    pub name: String,
    pub password: String,
    pub enabled: bool,
    pub is_admin: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct UserKeyQuery {
    pub id: Scope<i64>,
    pub user_id: Scope<i64>,
    pub api_key: Scope<String>,
    pub enabled: Scope<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserKeyQueryRow {
    pub id: i64,
    pub user_id: i64,
    pub api_key: String,
    pub label: Option<String>,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserKeyMemoryRow {
    pub id: i64,
    pub user_id: i64,
    pub api_key: String,
    pub label: Option<String>,
    pub enabled: bool,
}
