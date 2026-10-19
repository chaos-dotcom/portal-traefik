
//! Backend-agnostic storage interface — port of `portal.storage.base`.

pub mod frameio_backend;

use std::collections::HashMap;

use serde_json::Value;

#[derive(Clone, Debug)]
pub struct DestinationConfig {
    pub r#type: String,
    pub data: Value,
}

impl DestinationConfig {
    pub fn new(r#type: &str, data: Value) -> Self {
        Self { r#type: r#type.to_string(), data }
    }
    /// Value of a required string field.
    pub fn require(&self, key: &str) -> Result<String, String> {
        match self.data.get(key).and_then(|v| v.as_str()) {
            Some(s) if !s.is_empty() => Ok(s.to_string()),
            _ => Err(format!("destination config missing '{key}' for type '{}'", self.r#type)),
        }
    }
}

#[derive(Clone, Debug)]
#[allow(dead_code)] // mirrors portal.storage.base.RemoteFile; not every field is read by Frame.io's flows
pub struct RemoteFile {
    pub id: String,
    pub name: String,
    pub size: Option<i64>,
    pub media_type: Option<String>,
    pub parent_id: Option<String>,
    pub project_id: Option<String>,
    /// POSIX path of the file's folder relative to the rule's root source folder (root exclusive),
    /// e.g. "Dailies/Monday". Empty for files directly in the root. The sync engine prepends it to
    /// the local path so the source's subfolder tree is mirrored onto the destination. `None` when
    /// the file is not under the rule's root folder at all (it must not be synced).
    pub relative_dir: Option<String>,
    pub status: Option<String>,
}

impl Default for RemoteFile {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            size: None,
            media_type: None,
            parent_id: None,
            project_id: None,
            relative_dir: Some(String::new()), // Python's default is "" (in the root), not None
            status: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct UploadSession {
    pub id: String,
    pub destination: DestinationConfig,
    pub filename: String,
    pub size: i64,
    pub backend_data: Value,
}

#[derive(Clone, Debug)]
pub struct UploadChunk {
    pub part: i64,
    pub size: i64,
    pub url: String,
}

#[derive(Clone, Debug)]
#[allow(dead_code)] // mirrors portal.storage.base.UploadCredentials
pub struct UploadCredentials {
    pub upload_session_id: String,
    pub chunks: Vec<UploadChunk>,
    pub headers: HashMap<String, String>,
}

#[derive(Clone, Debug)]
pub struct DownloadURL {
    pub url: String,
}

#[derive(Clone, Debug)]
pub struct ChangeSubscription {
    pub id: String,
    pub secret: Option<String>,
}

