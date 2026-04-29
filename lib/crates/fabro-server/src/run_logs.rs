use std::path::Path;
use std::sync::Arc;

use anyhow::Context as _;
use object_store::ObjectStore;
use object_store::path::Path as ObjectStorePath;

pub(crate) fn durable_run_log_path(
    prefix: &str,
    run_id: &fabro_types::RunId,
) -> ObjectStorePath {
    ObjectStorePath::from(format!("{prefix}/{run_id}/server.log"))
}

pub(crate) async fn archive_terminal_run_log(
    store: Arc<dyn ObjectStore>,
    prefix: &str,
    run_id: &fabro_types::RunId,
    local_path: &Path,
) -> anyhow::Result<()> {
    let payload = tokio::fs::read(local_path)
        .await
        .with_context(|| format!("reading terminal run log {}", local_path.display()))?;
    store
        .put(&durable_run_log_path(prefix, run_id), payload.into())
        .await
        .context("writing durable terminal run log")?;
    Ok(())
}

pub(crate) async fn read_durable_run_log(
    store: Arc<dyn ObjectStore>,
    prefix: &str,
    run_id: &fabro_types::RunId,
) -> anyhow::Result<Option<bytes::Bytes>> {
    let path = durable_run_log_path(prefix, run_id);
    match store.get(&path).await {
        Ok(result) => Ok(Some(result.bytes().await?)),
        Err(object_store::Error::NotFound { .. }) => Ok(None),
        Err(err) => Err(err.into()),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bytes::Bytes;
    use object_store::memory::InMemory;
    use object_store::path::Path;
    use object_store::ObjectStore;

    use super::{archive_terminal_run_log, durable_run_log_path};
    use fabro_types::RunId;

    #[tokio::test]
    async fn archive_terminal_run_log_writes_blob_object() {
        let temp = tempfile::tempdir().unwrap();
        let local_path = temp.path().join("runtime/server.log");
        tokio::fs::create_dir_all(local_path.parent().unwrap())
            .await
            .unwrap();
        tokio::fs::write(&local_path, b"worker log line\n")
            .await
            .unwrap();

        let run_id = RunId::new();
        let store: Arc<dyn ObjectStore> = Arc::new(InMemory::new());

        archive_terminal_run_log(Arc::clone(&store), "run-logs", &run_id, &local_path)
            .await
            .unwrap();

        let bytes = store
            .get(&durable_run_log_path("run-logs", &run_id))
            .await
            .unwrap()
            .bytes()
            .await
            .unwrap();

        assert_eq!(bytes, Bytes::from_static(b"worker log line\n"));
    }

    #[test]
    fn durable_run_log_path_uses_run_logs_prefix() {
        let run_id: RunId = "01JT56VE4Z5NZ814GZN2JZD65A".parse().unwrap();
        assert_eq!(
            durable_run_log_path("run-logs", &run_id),
            Path::from("run-logs/01JT56VE4Z5NZ814GZN2JZD65A/server.log")
        );
    }
}
