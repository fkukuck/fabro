use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use fabro_http::{HttpClient, HttpClientBuilder};

use crate::azure::protocol::{
    ExecRequest, ExecResponse, ReadFileRequest, ReadFileResponse, WriteFileRequest,
};

pub struct SandboxdClient {
    http:     HttpClient,
    base_url: String,
}

impl SandboxdClient {
    pub fn new(base_url: String) -> Result<Self, String> {
        let http = HttpClientBuilder::new()
            .build()
            .map_err(|err| err.to_string())?;
        Ok(Self { http, base_url })
    }

    pub async fn exec(&self, request: ExecRequest) -> Result<ExecResponse, String> {
        self.http
            .post(format!("{}/exec", self.base_url))
            .json(&request)
            .send()
            .await
            .map_err(|err| err.to_string())?
            .error_for_status()
            .map_err(|err| err.to_string())?
            .json()
            .await
            .map_err(|err| err.to_string())
    }

    pub async fn read_file(&self, path: &str) -> Result<Vec<u8>, String> {
        let response: ReadFileResponse = self
            .http
            .post(format!("{}/read-file", self.base_url))
            .json(&ReadFileRequest {
                path: path.to_string(),
            })
            .send()
            .await
            .map_err(|err| err.to_string())?
            .error_for_status()
            .map_err(|err| err.to_string())?
            .json()
            .await
            .map_err(|err| err.to_string())?;
        STANDARD
            .decode(response.content_base64)
            .map_err(|err| err.to_string())
    }

    pub async fn write_file(&self, path: &str, bytes: &[u8]) -> Result<(), String> {
        self.http
            .post(format!("{}/write-file", self.base_url))
            .json(&WriteFileRequest {
                path:           path.to_string(),
                content_base64: STANDARD.encode(bytes),
            })
            .send()
            .await
            .map_err(|err| err.to_string())?
            .error_for_status()
            .map_err(|err| err.to_string())?;
        Ok(())
    }

    pub async fn health(&self) -> Result<(), String> {
        self.http
            .get(format!("{}/health", self.base_url))
            .send()
            .await
            .map_err(|err| err.to_string())?
            .error_for_status()
            .map_err(|err| err.to_string())?;
        Ok(())
    }
}
