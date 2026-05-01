//! Azure sandbox support.

use std::collections::HashMap;
use std::fmt::Write;
use std::path::Path;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use fabro_config::Storage;
use fabro_github::GitHubCredentials;
use fabro_static::EnvVars;
use fabro_types::{CommandTermination, RunId};
use tokio::sync::OnceCell;
use tokio::{fs, time};
use tokio_util::sync::CancellationToken;

use crate::azure::arm::{AzureArmClient, ContainerGroupView};
use crate::azure::config::AzurePlatformConfig;
use crate::azure::protocol::{ExecRequest, ExecResponse};
use crate::azure::resource_id::ContainerGroupResourceId;
use crate::config::AzureConfig;
use crate::repo::resolve_clone_source;
use crate::{
    DirEntry, ExecResult, GrepOptions, Sandbox, SandboxEvent, SandboxEventCallback,
    format_lines_numbered, git_push_via_exec, setup_git_via_exec, shell_quote,
};

pub mod arm;
pub mod config;
pub mod protocol;
pub mod resource_id;
pub mod sandboxd_client;

use sandboxd_client::SandboxdClient;

const WORKING_DIRECTORY: &str = "/workspace";
const DEFAULT_CPU: f64 = 2.0;
const DEFAULT_MEMORY_GB: f64 = 4.0;
const SANDBOXD_READY_TIMEOUT: Duration = Duration::from_mins(3);

pub struct AzureSandbox {
    runtime:          AzureConfig,
    platform:         AzurePlatformConfig,
    arm:              AzureArmClient,
    sandboxd:         OnceCell<SandboxdClient>,
    resource_id:      OnceCell<ContainerGroupResourceId>,
    github_app:       Option<GitHubCredentials>,
    run_id:           Option<RunId>,
    clone_origin_url: Option<String>,
    clone_branch:     Option<String>,
    origin_url:       OnceCell<String>,
    event_callback:   Option<SandboxEventCallback>,
}

impl AzureSandbox {
    pub fn new(
        runtime: AzureConfig,
        github_app: Option<GitHubCredentials>,
        run_id: Option<RunId>,
        clone_origin_url: Option<String>,
        clone_branch: Option<String>,
    ) -> Result<Self, String> {
        let platform = load_platform_from_worker_storage_root()?;
        let arm = AzureArmClient::new(platform.clone())?;
        Ok(Self {
            runtime,
            platform,
            arm,
            sandboxd: OnceCell::new(),
            resource_id: OnceCell::new(),
            github_app,
            run_id,
            clone_origin_url,
            clone_branch,
            origin_url: OnceCell::new(),
            event_callback: None,
        })
    }

    pub fn reconnect(resource_id: &str, storage_root: &Path) -> Result<Self, String> {
        let platform = load_platform_from_storage_root(storage_root)?;
        let arm = AzureArmClient::new(platform.clone())?;
        let parsed = ContainerGroupResourceId::parse(resource_id)?;
        let resource_id_cell = OnceCell::new();
        resource_id_cell
            .set(parsed)
            .map_err(|_| "Azure sandbox already has a resource ID".to_string())?;
        Ok(Self {
            runtime: AzureConfig::default(),
            platform,
            arm,
            sandboxd: OnceCell::new(),
            resource_id: resource_id_cell,
            github_app: None,
            run_id: None,
            clone_origin_url: None,
            clone_branch: None,
            origin_url: OnceCell::new(),
            event_callback: None,
        })
    }

    pub fn set_event_callback(&mut self, cb: SandboxEventCallback) {
        self.event_callback = Some(cb);
    }

    fn emit(&self, event: SandboxEvent) {
        event.trace();
        if let Some(ref cb) = self.event_callback {
            cb(event);
        }
    }

    fn resolve_path(path: &str) -> String {
        if Path::new(path).is_absolute() {
            path.to_string()
        } else {
            format!("{WORKING_DIRECTORY}/{path}")
        }
    }

    fn exec_working_dir(working_dir: Option<&str>) -> String {
        working_dir.map_or_else(|| WORKING_DIRECTORY.to_string(), Self::resolve_path)
    }

    fn sandbox_name(&self) -> String {
        if let Some(run_id) = self.run_id {
            return format!("fabro-{}", run_id.to_string().to_lowercase());
        }

        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        format!("fabro-{millis}")
    }

    fn resource_id(&self) -> Result<&ContainerGroupResourceId, String> {
        self.resource_id
            .get()
            .ok_or_else(|| "Azure sandbox not initialized".to_string())
    }

    fn validate_platform_for_creation(&self) -> Result<(), String> {
        if self.platform.acr_identity_resource_id.trim().is_empty() {
            return Err(
                "Azure platform config is missing acr_identity_resource_id; this looks like an old snapshot and cannot be used to create a new Azure sandbox"
                    .to_string(),
            );
        }

        Ok(())
    }

    async fn sandboxd_client(&self) -> Result<&SandboxdClient, String> {
        self.sandboxd
            .get_or_try_init(|| async {
                let base_url = self.discover_sandboxd_base_url().await?;
                let client = SandboxdClient::new(base_url)?;
                client.health().await?;
                Ok(client)
            })
            .await
    }

    async fn wait_for_sandboxd(&self) -> Result<(), String> {
        let deadline = Instant::now() + SANDBOXD_READY_TIMEOUT;
        let mut last_error = "sandboxd not ready".to_string();

        while Instant::now() < deadline {
            match self.sandboxd_client().await {
                Ok(_) => return Ok(()),
                Err(err) => last_error = err,
            }
            time::sleep(Duration::from_secs(1)).await;
        }

        Err(format!("Timed out waiting for sandboxd: {last_error}"))
    }

    async fn discover_sandboxd_base_url(&self) -> Result<String, String> {
        let view = self.arm.get_container_group(self.resource_id()?).await?;
        container_group_base_url(&view, self.platform.sandboxd_port)
    }

    fn exec_result_from_response(response: ExecResponse) -> ExecResult {
        let termination = if response.timed_out {
            CommandTermination::TimedOut
        } else {
            CommandTermination::Exited
        };

        ExecResult {
            stdout: response.stdout,
            stderr: response.stderr,
            exit_code: (termination == CommandTermination::Exited).then_some(response.exit_code),
            termination,
            duration_ms: response.duration_ms,
        }
    }

    async fn ensure_workspace_clone(&self) -> crate::Result<()> {
        if self
            .file_exists(&format!("{WORKING_DIRECTORY}/.git"))
            .await?
        {
            return Ok(());
        }

        let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        let Ok((detected_url, detected_branch)) = resolve_clone_source(
            self.clone_origin_url.as_deref(),
            self.clone_branch.as_deref(),
            &cwd,
        ) else {
            let mkdir = format!("mkdir -p {}", shell_quote(WORKING_DIRECTORY));
            let result = self.exec_command(&mkdir, 10_000, None, None, None).await?;
            if !result.is_success() {
                return Err(crate::Error::message(format!(
                    "failed to create working directory (exit {}): {}",
                    result.display_exit_code(), result.stderr
                )));
            }
            return Ok(());
        };

        let url = fabro_github::ssh_url_to_https(&detected_url);
        let branch = detected_branch;
        self.emit(SandboxEvent::GitCloneStarted {
            url:    url.clone(),
            branch: branch.clone(),
        });
        let clone_start = Instant::now();

        let auth_url = match &self.github_app {
            Some(creds) => Some(
                fabro_github::resolve_authenticated_url(
                    &fabro_github::GitHubContext::new(creds, &fabro_github::github_api_base_url()),
                    &url,
                )
                .await
                .map_err(|e| format!("Failed to get GitHub App credentials for clone: {e}"))?,
            ),
            None => None,
        };
        let clone_url = auth_url
            .as_ref()
            .map_or(url.as_str(), |url| url.as_raw_url().as_str());

        let mut clone_cmd = "git clone --recursive".to_string();
        if let Some(branch) = &branch {
            let _ = write!(clone_cmd, " --branch {}", shell_quote(branch));
        }
        let _ = write!(
            clone_cmd,
            " {} {}",
            shell_quote(clone_url),
            shell_quote(WORKING_DIRECTORY)
        );

        let clone_result = self
            .exec_command(&clone_cmd, 120_000, None, None, None)
            .await?;
        if !clone_result.is_success() {
            let err = format!(
                "Failed to clone repo into Azure sandbox (exit {}): {}",
                clone_result.display_exit_code(), clone_result.stderr
            );
            self.emit(SandboxEvent::GitCloneFailed {
                url,
                error: err.clone(),
                causes: Vec::new(),
            });
            return Err(err.into());
        }

        let _ = self.origin_url.set(url.clone());
        self.emit(SandboxEvent::GitCloneCompleted {
            url,
            duration_ms: u64::try_from(clone_start.elapsed().as_millis()).unwrap_or(u64::MAX),
        });
        Ok(())
    }

    async fn exec_via_sandboxd(
        &self,
        command: &str,
        timeout_ms: u64,
        working_dir: Option<&str>,
        env_vars: Option<&HashMap<String, String>>,
        cancel_token: Option<CancellationToken>,
    ) -> Result<ExecResult, String> {
        let client = self.sandboxd_client().await?;
        let request = ExecRequest {
            command: command.to_string(),
            working_dir: Some(Self::exec_working_dir(working_dir)),
            env: env_vars.cloned().unwrap_or_default(),
            timeout_ms,
        };

        let response = if let Some(token) = cancel_token {
            tokio::select! {
                () = token.cancelled() => return Err("Command cancelled".to_string()),
                result = client.exec(request) => result?,
            }
        } else {
            client.exec(request).await?
        };

        Ok(Self::exec_result_from_response(response))
    }
}

fn load_platform_from_worker_storage_root() -> Result<AzurePlatformConfig, String> {
    let storage_root = std::env::var(EnvVars::FABRO_STORAGE_ROOT).map_err(|_| {
        format!(
            "{} is required to load Azure platform config",
            EnvVars::FABRO_STORAGE_ROOT
        )
    })?;
    load_platform_from_storage_root(Path::new(&storage_root))
}

fn load_platform_from_storage_root(storage_root: &Path) -> Result<AzurePlatformConfig, String> {
    let path = Storage::new(storage_root)
        .runtime_directory()
        .azure_platform_config_path();
    AzurePlatformConfig::load_from_path(&path)
}

#[async_trait]
impl Sandbox for AzureSandbox {
    async fn read_file(
        &self,
        path: &str,
        offset: Option<usize>,
        limit: Option<usize>,
    ) -> crate::Result<String> {
        let bytes = self
            .sandboxd_client()
            .await?
            .read_file(&Self::resolve_path(path))
            .await?;
        let content = String::from_utf8_lossy(&bytes);
        Ok(format_lines_numbered(&content, offset, limit))
    }

    async fn write_file(&self, path: &str, content: &str) -> crate::Result<()> {
        self.sandboxd_client()
            .await?
            .write_file(&Self::resolve_path(path), content.as_bytes())
            .await
            .map_err(Into::into)
    }

    async fn delete_file(&self, path: &str) -> crate::Result<()> {
        let cmd = format!("rm -rf {}", shell_quote(&Self::resolve_path(path)));
        let result = self.exec_command(&cmd, 10_000, None, None, None).await?;
        if result.is_success() {
            Ok(())
        } else {
            Err(crate::Error::message(format!(
                "delete failed (exit {}): {}",
                result.display_exit_code(), result.stderr
            )))
        }
    }

    async fn file_exists(&self, path: &str) -> crate::Result<bool> {
        let cmd = format!("test -e {}", shell_quote(&Self::resolve_path(path)));
        let result = self.exec_command(&cmd, 10_000, None, None, None).await?;
        Ok(result.is_success())
    }

    async fn list_directory(
        &self,
        path: &str,
        depth: Option<usize>,
    ) -> crate::Result<Vec<DirEntry>> {
        let resolved = Self::resolve_path(path);
        let max_depth = depth.unwrap_or(1);
        let cmd = format!(
            "find {} -mindepth 1 -maxdepth {} \\( -type f -o -type d \\) -printf '%P\\t%y\\t%s\\n'",
            shell_quote(&resolved),
            max_depth
        );
        let result = self.exec_command(&cmd, 30_000, None, None, None).await?;
        if !result.is_success() {
            return Err(crate::Error::message(format!(
                "list_directory failed (exit {}): {}",
                result.display_exit_code(), result.stderr
            )));
        }

        Ok(result
            .stdout
            .lines()
            .filter(|line| !line.is_empty())
            .map(|line| {
                let mut parts = line.splitn(3, '\t');
                let name = parts.next().unwrap_or_default().to_string();
                let kind = parts.next().unwrap_or("f");
                let size = parts.next().and_then(|value| value.parse::<u64>().ok());
                DirEntry {
                    name,
                    is_dir: kind == "d",
                    size,
                }
            })
            .collect())
    }

    async fn exec_command(
        &self,
        command: &str,
        timeout_ms: u64,
        working_dir: Option<&str>,
        env_vars: Option<&HashMap<String, String>>,
        cancel_token: Option<CancellationToken>,
    ) -> crate::Result<ExecResult> {
        self.exec_via_sandboxd(command, timeout_ms, working_dir, env_vars, cancel_token)
            .await
            .map_err(Into::into)
    }

    async fn grep(
        &self,
        pattern: &str,
        path: &str,
        options: &GrepOptions,
    ) -> crate::Result<Vec<String>> {
        let resolved = Self::resolve_path(path);
        let mut rg_cmd = "rg --line-number --no-heading".to_string();
        if options.case_insensitive {
            rg_cmd.push_str(" -i");
        }
        if let Some(glob_filter) = &options.glob_filter {
            let _ = write!(rg_cmd, " --glob {}", shell_quote(glob_filter));
        }
        if let Some(max) = options.max_results {
            let _ = write!(rg_cmd, " --max-count {max}");
        }
        let _ = write!(
            rg_cmd,
            " -- {} {}",
            shell_quote(pattern),
            shell_quote(&resolved)
        );

        let cmd = format!(
            "if command -v rg >/dev/null 2>&1; then {rg_cmd}; else grep -rn -- {} {}; fi",
            shell_quote(pattern),
            shell_quote(&resolved)
        );
        let result = self.exec_command(&cmd, 30_000, None, None, None).await?;
        if result.exit_code == Some(1) {
            return Ok(Vec::new());
        }
        if !result.is_success() {
            return Err(crate::Error::message(format!(
                "grep failed (exit {}): {}",
                result.display_exit_code(), result.stderr
            )));
        }
        Ok(result.stdout.lines().map(String::from).collect())
    }

    async fn glob(&self, pattern: &str, path: Option<&str>) -> crate::Result<Vec<String>> {
        let base = path.map_or_else(|| WORKING_DIRECTORY.to_string(), Self::resolve_path);
        let cmd = format!(
            "find {} -name {} -type f | sort",
            shell_quote(&base),
            shell_quote(pattern)
        );
        let result = self.exec_command(&cmd, 30_000, None, None, None).await?;
        if !result.is_success() {
            return Err(crate::Error::message(format!(
                "glob failed (exit {}): {}",
                result.display_exit_code(), result.stderr
            )));
        }
        Ok(result
            .stdout
            .lines()
            .filter(|line| !line.is_empty())
            .map(String::from)
            .collect())
    }

    async fn download_file_to_local(
        &self,
        remote_path: &str,
        local_path: &Path,
    ) -> crate::Result<()> {
        let bytes = self
            .sandboxd_client()
            .await?
            .read_file(&Self::resolve_path(remote_path))
            .await?;
        if let Some(parent) = local_path.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|err| crate::Error::context("Failed to create local parent dirs", err))?;
        }
        fs::write(local_path, bytes).await.map_err(|err| {
            crate::Error::context(format!("Failed to write {}", local_path.display()), err)
        })
    }

    async fn upload_file_from_local(
        &self,
        local_path: &Path,
        remote_path: &str,
    ) -> crate::Result<()> {
        let bytes = fs::read(local_path).await.map_err(|err| {
            crate::Error::context(format!("Failed to read {}", local_path.display()), err)
        })?;
        self.sandboxd_client()
            .await?
            .write_file(&Self::resolve_path(remote_path), &bytes)
            .await
            .map_err(Into::into)
    }

    async fn initialize(&self) -> crate::Result<()> {
        self.emit(SandboxEvent::Initializing {
            provider: "azure".into(),
        });
        let init_start = Instant::now();

        let resource_id = if let Some(resource_id) = self.resource_id.get() {
            resource_id.clone()
        } else {
            self.validate_platform_for_creation()?;
            let image = self
                .runtime
                .image
                .clone()
                .ok_or_else(|| "Azure sandbox image is required".to_string())?;
            let resource_id = self
                .arm
                .create_container_group(
                    &self.sandbox_name(),
                    &image,
                    self.runtime.cpu.unwrap_or(DEFAULT_CPU),
                    self.runtime.memory_gb.unwrap_or(DEFAULT_MEMORY_GB),
                )
                .await
                .inspect_err(|err| {
                    let duration_ms =
                        u64::try_from(init_start.elapsed().as_millis()).unwrap_or(u64::MAX);
                    self.emit(SandboxEvent::InitializeFailed {
                        provider: "azure".into(),
                        error: err.clone(),
                        causes: Vec::new(),
                        duration_ms,
                    });
                })?;
            self.resource_id
                .set(resource_id.clone())
                .map_err(|_| "Azure sandbox already initialized".to_string())?;
            resource_id
        };

        if let Err(err) = self.wait_for_sandboxd().await {
            let duration_ms = u64::try_from(init_start.elapsed().as_millis()).unwrap_or(u64::MAX);
            self.emit(SandboxEvent::InitializeFailed {
                provider: "azure".into(),
                error: err.clone(),
                causes: Vec::new(),
                duration_ms,
            });
            return Err(err.into());
        }

        if let Err(err) = self.ensure_workspace_clone().await {
            let duration_ms = u64::try_from(init_start.elapsed().as_millis()).unwrap_or(u64::MAX);
            self.emit(SandboxEvent::InitializeFailed {
                provider: "azure".into(),
                error: err.to_string(),
                causes: err.causes(),
                duration_ms,
            });
            return Err(err);
        }

        let duration_ms = u64::try_from(init_start.elapsed().as_millis()).unwrap_or(u64::MAX);
        self.emit(SandboxEvent::Ready {
            provider: "azure".into(),
            duration_ms,
            name: Some(resource_id.container_group_name.clone()),
            cpu: self.runtime.cpu.or(Some(DEFAULT_CPU)),
            memory: self.runtime.memory_gb.or(Some(DEFAULT_MEMORY_GB)),
            url: None,
        });
        Ok(())
    }

    async fn cleanup(&self) -> crate::Result<()> {
        self.emit(SandboxEvent::CleanupStarted {
            provider: "azure".into(),
        });
        let start = Instant::now();
        if let Some(resource_id) = self.resource_id.get() {
            if let Err(err) = self.arm.delete_container_group(resource_id).await {
                self.emit(SandboxEvent::CleanupFailed {
                    provider: "azure".into(),
                    error:    err.clone(),
                    causes:   Vec::new(),
                });
                return Err(err.into());
            }
        }
        let duration_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
        self.emit(SandboxEvent::CleanupCompleted {
            provider: "azure".into(),
            duration_ms,
        });
        Ok(())
    }

    fn working_directory(&self) -> &str {
        WORKING_DIRECTORY
    }

    fn platform(&self) -> &'static str {
        "linux"
    }

    fn os_version(&self) -> String {
        "Linux (Azure)".to_string()
    }

    fn sandbox_info(&self) -> String {
        self.resource_id
            .get()
            .map(ToString::to_string)
            .unwrap_or_default()
    }

    async fn refresh_push_credentials(&self) -> crate::Result<()> {
        let Some(origin_url) = self.origin_url.get() else {
            return Ok(());
        };
        let Some(creds) = &self.github_app else {
            return Ok(());
        };

        let auth_url = fabro_github::resolve_authenticated_url(
            &fabro_github::GitHubContext::new(creds, &fabro_github::github_api_base_url()),
            origin_url,
        )
        .await
        .map_err(|e| format!("Failed to refresh GitHub App token: {e}"))?;
        let cmd = format!(
            "git -c maintenance.auto=0 remote set-url origin {}",
            shell_quote(auth_url.as_raw_url().as_str())
        );
        let result = self.exec_command(&cmd, 10_000, None, None, None).await?;
        if result.is_success() {
            Ok(())
        } else {
            Err(crate::Error::message(format!(
                "Failed to refresh push credentials (exit {}): {}",
                result.display_exit_code(), result.stderr
            )))
        }
    }

    async fn setup_git(
        &self,
        intent: &crate::GitSetupIntent,
    ) -> crate::Result<Option<crate::GitRunInfo>> {
        setup_git_via_exec(self, intent).await.map(Some)
    }

    fn resume_setup_commands(&self, run_branch: &str) -> Vec<String> {
        vec![format!(
            "git fetch origin {run_branch} && git checkout {run_branch}"
        )]
    }

    async fn git_push_ref(&self, refspec: &str) -> crate::Result<()> {
        git_push_via_exec(self, refspec).await
    }

    fn parallel_worktree_path(
        &self,
        _run_dir: &std::path::Path,
        run_id: &str,
        node_id: &str,
        key: &str,
    ) -> String {
        format!(
            "{}/.fabro/scratch/{}/parallel/{}/{}",
            self.working_directory(),
            run_id,
            node_id,
            key
        )
    }

    fn origin_url(&self) -> Option<&str> {
        self.origin_url.get().map(String::as_str)
    }
}

fn container_group_base_url(
    view: &ContainerGroupView,
    sandboxd_port: u16,
) -> Result<String, String> {
    let host = view
        .properties
        .ip_address
        .as_ref()
        .and_then(|ip_address| {
            ip_address.fqdn.clone().or_else(|| {
                ip_address
                    .ip
                    .as_deref()
                    .filter(|ip| *ip != "0.0.0.0")
                    .map(str::to_string)
            })
        })
        .ok_or_else(|| "container group has no reachable IP address yet".to_string())?;
    Ok(format!("http://{host}:{sandboxd_port}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::azure::arm::{ContainerGroupIpAddress, ContainerGroupPropertiesView};
    use crate::azure::protocol::ExecResponse;

    #[test]
    fn exec_working_dir_defaults_to_workspace() {
        assert_eq!(AzureSandbox::exec_working_dir(None), "/workspace");
    }

    #[test]
    fn container_group_base_url_rejects_unspecified_ip() {
        let view = ContainerGroupView {
            properties: ContainerGroupPropertiesView {
                ip_address: Some(ContainerGroupIpAddress {
                    ip:   Some("0.0.0.0".into()),
                    fqdn: None,
                }),
                ..ContainerGroupPropertiesView::default()
            },
            ..ContainerGroupView::default()
        };

        let err = container_group_base_url(&view, 7777).unwrap_err();
        assert!(err.contains("reachable IP address"));
    }

    #[test]
    fn sandbox_name_lowercases_run_ids_for_aci() {
        let run_id: RunId = "01KPJGM228CBVW27W2KRJE7NP1".parse().unwrap();
        let sandbox = AzureSandbox {
            runtime:          AzureConfig::default(),
            platform:         AzurePlatformConfig {
                subscription_id: "sub".into(),
                resource_group:  "rg".into(),
                location:        "loc".into(),
                subnet_id:       "subnet".into(),
                acr_server:      "acr.azurecr.io".into(),
                acr_identity_resource_id: "identity".into(),
                sandboxd_port:   7777,
            },
            arm:              AzureArmClient::new_with_base_url(
                fabro_http::http_client().unwrap(),
                AzurePlatformConfig {
                    subscription_id: "sub".into(),
                    resource_group:  "rg".into(),
                    location:        "loc".into(),
                    subnet_id:       "subnet".into(),
                    acr_server:      "acr.azurecr.io".into(),
                    acr_identity_resource_id: "identity".into(),
                    sandboxd_port:   7777,
                },
                "https://management.azure.com".into(),
            ),
            resource_id:      OnceCell::new(),
            sandboxd:         OnceCell::new(),
            run_id:           Some(run_id),
            github_app:       None,
            clone_origin_url: None,
            clone_branch:     None,
            origin_url:       OnceCell::new(),
            event_callback:   None,
        };

        assert_eq!(sandbox.sandbox_name(), "fabro-01kpjgm228cbvw27w2krje7np1");
    }

    #[test]
    fn exec_response_maps_to_exited_exec_result() {
        let result = AzureSandbox::exec_result_from_response(ExecResponse {
            stdout: "ok".into(),
            stderr: String::new(),
            exit_code: 0,
            timed_out: false,
            duration_ms: 42,
        });

        assert_eq!(result.stdout, "ok");
        assert_eq!(result.exit_code, Some(0));
        assert_eq!(result.termination, fabro_types::CommandTermination::Exited);
        assert_eq!(result.duration_ms, 42);
    }

    #[test]
    fn exec_response_maps_timeouts_to_missing_exit_code() {
        let result = AzureSandbox::exec_result_from_response(ExecResponse {
            stdout: String::new(),
            stderr: "timed out".into(),
            exit_code: 124,
            timed_out: true,
            duration_ms: 5000,
        });

        assert_eq!(result.exit_code, None);
        assert_eq!(result.termination, fabro_types::CommandTermination::TimedOut);
        assert_eq!(result.stderr, "timed out");
    }

    #[tokio::test]
    async fn initialize_rejects_missing_acr_identity_resource_id_before_arm_create() {
        let sandbox = AzureSandbox {
            runtime:          AzureConfig {
                image:     Some("fabro.azurecr.io/fabro-sandboxes/base:latest".into()),
                cpu:       Some(2.0),
                memory_gb: Some(4.0),
            },
            platform:         AzurePlatformConfig {
                subscription_id: "sub".into(),
                resource_group:  "rg".into(),
                location:        "loc".into(),
                subnet_id:       "subnet".into(),
                acr_server:      "acr.azurecr.io".into(),
                acr_identity_resource_id: String::new(),
                sandboxd_port:   7777,
            },
            arm:              AzureArmClient::new_with_base_url(
                fabro_http::test_http_client().unwrap(),
                AzurePlatformConfig {
                    subscription_id: "sub".into(),
                    resource_group:  "rg".into(),
                    location:        "loc".into(),
                    subnet_id:       "subnet".into(),
                    acr_server:      "acr.azurecr.io".into(),
                    acr_identity_resource_id: String::new(),
                    sandboxd_port:   7777,
                },
                "http://127.0.0.1:1".into(),
            ),
            resource_id:      OnceCell::new(),
            sandboxd:         OnceCell::new(),
            run_id:           None,
            github_app:       None,
            clone_origin_url: None,
            clone_branch:     None,
            origin_url:       OnceCell::new(),
            event_callback:   None,
        };

        let err = sandbox.initialize().await.unwrap_err().to_string();
        assert!(err.contains("acr_identity_resource_id"));
        assert!(err.contains("old snapshot") || err.contains("legacy snapshot"));
    }
}
