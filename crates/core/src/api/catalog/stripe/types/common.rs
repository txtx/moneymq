use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListResponse<T> {
    pub object: String,
    pub data: Vec<T>,
    pub has_more: bool,
    pub url: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListParams {
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(default)]
    pub starting_after: Option<String>,
    #[serde(default)]
    pub product: Option<String>,
}
