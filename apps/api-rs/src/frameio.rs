
//! Frame.io seam.
//!
//! The full client (`portal.frameio.client`, ~830 lines: Adobe IMS token refresh, retry/429
//! handling, pickers, uploads, downloads, webhooks, shares) is **not ported yet**. What exists
//! here is the error taxonomy and the value types the rest of the code needs, plus a client that
//! reports "not connected" — which is exactly what the Python client does when no connection row
//! exists. Call sites are written against this seam so the real client drops in unchanged.

#[derive(Debug, thiserror::Error)]
pub enum FrameioError {
    #[error("no usable Frame.io connection")]
    NotConnected,
    #[error("rate limited")]
    RateLimited,
    #[error("not found")]
    NotFound,
    #[error("forbidden")]
    Forbidden,
    #[error("{0}")]
    Other(String),
}

#[allow(dead_code)] // used once the real client and picker routes land
#[derive(Clone, Debug)]
pub struct PickerItem {
    pub id: String,
    pub name: String,
    pub parent_id: Option<String>,
    pub project_id: Option<String>,
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct ProjectItem {
    pub id: String,
    pub name: String,
    pub root_folder_id: Option<String>,
    pub workspace_id: Option<String>,
}

pub struct FrameioClient;

impl FrameioClient {
    /// Mirrors `get_frameio_client()` when there is no connection: every call fails NotConnected.
    pub async fn folder_ancestry(&self, _account_id: &str, _folder_id: &str) -> Result<Vec<PickerItem>, FrameioError> {
        Err(FrameioError::NotConnected)
    }
    pub async fn get_project(&self, _account_id: &str, _project_id: &str) -> Result<ProjectItem, FrameioError> {
        Err(FrameioError::NotConnected)
    }
    pub async fn get_folder(&self, _account_id: &str, _folder_id: &str) -> Result<PickerItem, FrameioError> {
        Err(FrameioError::NotConnected)
    }
}

pub fn get_frameio_client() -> FrameioClient {
    FrameioClient
}
