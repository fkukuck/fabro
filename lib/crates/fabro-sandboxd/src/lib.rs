use std::path::Path;
use std::time::Instant;

use axum::Router;
use axum::extract::Json;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use fabro_sandbox::azure::protocol::{
    ExecRequest, ExecResponse, ReadFileRequest, ReadFileResponse, WriteFileRequest,
};
use tokio::process::Command;
use tokio::{fs, time};

pub fn build_router() -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/exec", post(exec))
        .route("/read-file", post(read_file))
        .route("/write-file", post(write_file))
}

async fn health() -> StatusCode {
    StatusCode::OK
}

async fn exec(
    Json(request): Json<ExecRequest>,
) -> Result<Json<ExecResponse>, (StatusCode, String)> {
    let started = Instant::now();
    let mut command = Command::new("bash");
    command.arg("-lc").arg(&request.command);

    if let Some(working_dir) = &request.working_dir {
        command.current_dir(working_dir);
    }
    command.envs(&request.env);

    let output = match time::timeout(
        std::time::Duration::from_millis(request.timeout_ms),
        command.output(),
    )
    .await
    {
        Ok(result) => {
            let output =
                result.map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;
            ExecResponse {
                stdout:      String::from_utf8_lossy(&output.stdout).into_owned(),
                stderr:      String::from_utf8_lossy(&output.stderr).into_owned(),
                exit_code:   output.status.code().unwrap_or(-1),
                timed_out:   false,
                duration_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
            }
        }
        Err(_) => ExecResponse {
            stdout:      String::new(),
            stderr:      "command timed out".to_string(),
            exit_code:   -1,
            timed_out:   true,
            duration_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        },
    };

    Ok(Json(output))
}

async fn read_file(
    Json(request): Json<ReadFileRequest>,
) -> Result<Json<ReadFileResponse>, (StatusCode, String)> {
    let content = fs::read(&request.path)
        .await
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;
    Ok(Json(ReadFileResponse {
        content_base64: STANDARD.encode(content),
    }))
}

async fn write_file(
    Json(request): Json<WriteFileRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let bytes = STANDARD
        .decode(request.content_base64)
        .map_err(|err| (StatusCode::BAD_REQUEST, err.to_string()))?;

    if let Some(parent) = Path::new(&request.path).parent() {
        fs::create_dir_all(parent)
            .await
            .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;
    }

    fs::write(&request.path, bytes)
        .await
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;

    Ok(StatusCode::NO_CONTENT)
}
