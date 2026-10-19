//! Files over the data plane's binary-safe `files` endpoint, with the
//! operations it does not have (metadata, listing, rename, permissions)
//! derived from exec.

use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use sandbox_driver::{
    DerivedFs, DirEntry, Error, Exec, FileMetadata, Filesystem, ResourceKind, Result,
};

use crate::client::{AcaClient, Body, Replay, SendError};

pub(crate) struct AcaFs {
    client:     AcaClient,
    sandbox_id: String,
    derived:    DerivedFs,
}

impl AcaFs {
    pub(crate) fn new(client: AcaClient, sandbox_id: String, exec: Arc<dyn Exec>) -> Self {
        Self {
            client,
            sandbox_id,
            derived: DerivedFs::new(exec),
        }
    }

    fn files_path(&self) -> String {
        format!("/sandboxes/{}/files", self.sandbox_id)
    }

    fn error(error: SendError, path: &str) -> Error {
        error.into_error(ResourceKind::File, path)
    }
}

#[async_trait]
impl Filesystem for AcaFs {
    async fn read(&self, path: &str) -> Result<Vec<u8>> {
        self.client
            .data(
                reqwest::Method::GET,
                &self.files_path(),
                &[("path", path)],
                Body::None,
                Replay::Safe,
            )
            .await
            .map_err(|error| Self::error(error, path))
    }

    async fn read_range(&self, path: &str, offset: u64, length: Option<u64>) -> Result<Vec<u8>> {
        self.derived.read_range(path, offset, length).await
    }

    async fn write(&self, path: &str, content: &[u8]) -> Result<()> {
        self.client
            .data(
                reqwest::Method::PUT,
                &self.files_path(),
                &[("path", path), ("createDirs", "true")],
                Body::Bytes(content.to_vec()),
                Replay::Safe,
            )
            .await
            .map(|_| ())
            .map_err(|error| Self::error(error, path))
    }

    async fn write_append(&self, path: &str, content: &[u8]) -> Result<()> {
        self.derived.write_append(path, content).await
    }

    async fn delete(&self, path: &str, recursive: bool) -> Result<()> {
        self.derived.delete(path, recursive).await
    }

    async fn exists(&self, path: &str) -> Result<bool> {
        self.derived.exists(path).await
    }

    async fn metadata(&self, path: &str) -> Result<FileMetadata> {
        match self.derived.metadata(path).await {
            Err(Error::Exec(_)) if !self.derived.exists(path).await.unwrap_or(true) => {
                Err(Error::NotFound {
                    resource: ResourceKind::File,
                    id:       path.to_owned(),
                })
            }
            other => other,
        }
    }

    async fn list_dir(&self, path: &str, depth: usize) -> Result<Vec<DirEntry>> {
        self.derived.list_dir(path, depth).await
    }

    async fn create_dir(&self, path: &str) -> Result<()> {
        self.derived.create_dir(path).await
    }

    async fn rename(&self, from: &str, to: &str) -> Result<()> {
        self.derived.rename(from, to).await
    }

    async fn set_permissions(&self, path: &str, mode: u32) -> Result<()> {
        self.derived.set_permissions(path, mode).await
    }

    async fn upload(&self, local: &Path, remote: &str) -> Result<()> {
        let content = tokio::fs::read(local)
            .await
            .map_err(|error| Error::io(format!("reading {}", local.display()), error))?;
        self.write(remote, &content).await
    }

    async fn download(&self, remote: &str, local: &Path) -> Result<()> {
        let content = self.read(remote).await?;
        if let Some(parent) = local.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|error| {
                Error::io(format!("creating parent of {}", local.display()), error)
            })?;
        }
        tokio::fs::write(local, content)
            .await
            .map_err(|error| Error::io(format!("writing {}", local.display()), error))
    }
}
