//! Azure sandbox support.

use std::collections::HashMap;
use std::fmt::Write;
use std::path::Path;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use fabro_github::GitHubCredentials;
use fabro_types::RunId;
use tokio::sync::OnceCell;
use tokio::{fs, time};
use tokio_util::sync::CancellationToken;

use crate::azure::arm::{AzureArmClient, ContainerGroupView};
use crate::azure::config::AzurePlatformConfig;
use crate::azure::protocol::ExecRequest;
use crate::azure::resource_id::ContainerGroupResourceId;
use crate::config::AzureConfig;
use crate::repo::detect_repo_info;
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

pub struct AzureSandbox {
    runtime:        AzureConfig,
    platform:       AzurePlatformConfig,
    arm:            AzureArmClient,
    sandboxd:       OnceCell<SandboxdClient>,
    resource_id:    OnceCell<ContainerGroupResourceId>,
    github_app:     Option<GitHubCredentials>,
    run_id:         Option<RunId>,
    clone_branch:   Option<String>,
    origin_url:     OnceCell<String>,
    event_callback: Option<SandboxEventCallback>,
}

impl AzureSandbox {
    pub fn new(
        runtime: AzureConfig,
        github_app: Option<GitHubCredentials>,
        run_id: Option<RunId>,
        clone_branch: Option<String>,
    ) -> Result<Self, String> {
        let platform = AzurePlatformConfig::from_env()?;
        let arm = AzureArmClient::new(platform.clone())?;
        Ok(Self {
            runtime,
            platform,
            arm,
            sandboxd: OnceCell::new(),
            resource_id: OnceCell::new(),
            github_app,
            run_id,
            clone_branch,
            origin_url: OnceCell::new(),
            event_callback: None,
        })
    }

    pub fn reconnect(resource_id: &str) -> Result<Self, String> {
        let platform = AzurePlatformConfig::from_env()?;
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

    fn sandbox_name(&self) -> String {
        if let Some(run_id) = self.run_id {
            return format!("fabro-{run_id}");
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
        let deadline = Instant::now() + Duration::from_mins(1);
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

    async fn ensure_workspace_clone(&self) -> Result<(), String> {
        if self
            .file_exists(&format!("{WORKING_DIRECTORY}/.git"))
            .await?
        {
            return Ok(());
        }

        let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        let Ok((detected_url, detected_branch)) = detect_repo_info(&cwd) else {
            let mkdir = format!("mkdir -p {}", shell_quote(WORKING_DIRECTORY));
            let result = self.exec_command(&mkdir, 10_000, None, None, None).await?;
            if result.exit_code != 0 {
                return Err(format!(
                    "failed to create working directory (exit {}): {}",
                    result.exit_code, result.stderr
                ));
            }
            return Ok(());
        };

        let url = fabro_github::ssh_url_to_https(&detected_url);
        let branch = self.clone_branch.clone().or(detected_branch);
        self.emit(SandboxEvent::GitCloneStarted {
            url:    url.clone(),
            branch: branch.clone(),
        });
        let clone_start = Instant::now();

        let auth_url = match &self.github_app {
            Some(creds) => Some(
                fabro_github::resolve_authenticated_url(
                    &fabro_github::GitHubContext::new(
                        creds,
                        &fabro_github::github_api_base_url(),
                    ),
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
        if clone_result.exit_code != 0 {
            let err = format!(
                "Failed to clone repo into Azure sandbox (exit {}): {}",
                clone_result.exit_code, clone_result.stderr
            );
            self.emit(SandboxEvent::GitCloneFailed {
                url,
                error: err.clone(),
            });
            return Err(err);
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
            working_dir: working_dir.map(Self::resolve_path),
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

        Ok(ExecResult {
            stdout:      response.stdout,
            stderr:      response.stderr,
            exit_code:   response.exit_code,
            timed_out:   response.timed_out,
            duration_ms: response.duration_ms,
        })
    }
}

#[async_trait]
impl Sandbox for AzureSandbox {
    async fn read_file(
        &self,
        path: &str,
        offset: Option<usize>,
        limit: Option<usize>,
    ) -> Result<String, String> {
        let bytes = self
            .sandboxd_client()
            .await?
            .read_file(&Self::resolve_path(path))
            .await?;
        let content = String::from_utf8_lossy(&bytes);
        Ok(format_lines_numbered(&content, offset, limit))
    }

    async fn write_file(&self, path: &str, content: &str) -> Result<(), String> {
        self.sandboxd_client()
            .await?
            .write_file(&Self::resolve_path(path), content.as_bytes())
            .await
    }

    async fn delete_file(&self, path: &str) -> Result<(), String> {
        let cmd = format!("rm -rf {}", shell_quote(&Self::resolve_path(path)));
        let result = self.exec_command(&cmd, 10_000, None, None, None).await?;
        if result.exit_code == 0 {
            Ok(())
        } else {
            Err(format!(
                "delete failed (exit {}): {}",
                result.exit_code, result.stderr
            ))
        }
    }

    async fn file_exists(&self, path: &str) -> Result<bool, String> {
        let cmd = format!("test -e {}", shell_quote(&Self::resolve_path(path)));
        let result = self.exec_command(&cmd, 10_000, None, None, None).await?;
        Ok(result.exit_code == 0)
    }

    async fn list_directory(
        &self,
        path: &str,
        depth: Option<usize>,
    ) -> Result<Vec<DirEntry>, String> {
        let resolved = Self::resolve_path(path);
        let max_depth = depth.unwrap_or(1);
        let cmd = format!(
            "find {} -mindepth 1 -maxdepth {} \\( -type f -o -type d \\) -printf '%P\\t%y\\t%s\\n'",
            shell_quote(&resolved),
            max_depth
        );
        let result = self.exec_command(&cmd, 30_000, None, None, None).await?;
        if result.exit_code != 0 {
            return Err(format!(
                "list_directory failed (exit {}): {}",
                result.exit_code, result.stderr
            ));
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
    ) -> Result<ExecResult, String> {
        self.exec_via_sandboxd(command, timeout_ms, working_dir, env_vars, cancel_token)
            .await
    }

    async fn grep(
        &self,
        pattern: &str,
        path: &str,
        options: &GrepOptions,
    ) -> Result<Vec<String>, String> {
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
        if result.exit_code == 1 {
            return Ok(Vec::new());
        }
        if result.exit_code != 0 {
            return Err(format!(
                "grep failed (exit {}): {}",
                result.exit_code, result.stderr
            ));
        }
        Ok(result.stdout.lines().map(String::from).collect())
    }

    async fn glob(&self, pattern: &str, path: Option<&str>) -> Result<Vec<String>, String> {
        let base = path.map_or_else(|| WORKING_DIRECTORY.to_string(), Self::resolve_path);
        let cmd = format!(
            "find {} -name {} -type f | sort",
            shell_quote(&base),
            shell_quote(pattern)
        );
        let result = self.exec_command(&cmd, 30_000, None, None, None).await?;
        if result.exit_code != 0 {
            return Err(format!(
                "glob failed (exit {}): {}",
                result.exit_code, result.stderr
            ));
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
    ) -> Result<(), String> {
        let bytes = self
            .sandboxd_client()
            .await?
            .read_file(&Self::resolve_path(remote_path))
            .await?;
        if let Some(parent) = local_path.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|err| err.to_string())?;
        }
        fs::write(local_path, bytes)
            .await
            .map_err(|err| err.to_string())
    }

    async fn upload_file_from_local(
        &self,
        local_path: &Path,
        remote_path: &str,
    ) -> Result<(), String> {
        let bytes = fs::read(local_path).await.map_err(|err| err.to_string())?;
        self.sandboxd_client()
            .await?
            .write_file(&Self::resolve_path(remote_path), &bytes)
            .await
    }

    async fn initialize(&self) -> Result<(), String> {
        self.emit(SandboxEvent::Initializing {
            provider: "azure".into(),
        });
        let init_start = Instant::now();

        let resource_id = if let Some(resource_id) = self.resource_id.get() {
            resource_id.clone()
        } else {
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
                duration_ms,
            });
            return Err(err);
        }

        if let Err(err) = self.ensure_workspace_clone().await {
            let duration_ms = u64::try_from(init_start.elapsed().as_millis()).unwrap_or(u64::MAX);
            self.emit(SandboxEvent::InitializeFailed {
                provider: "azure".into(),
                error: err.clone(),
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

    async fn cleanup(&self) -> Result<(), String> {
        self.emit(SandboxEvent::CleanupStarted {
            provider: "azure".into(),
        });
        let start = Instant::now();
        if let Some(resource_id) = self.resource_id.get() {
            if let Err(err) = self.arm.delete_container_group(resource_id).await {
                self.emit(SandboxEvent::CleanupFailed {
                    provider: "azure".into(),
                    error:    err.clone(),
                });
                return Err(err);
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

    async fn refresh_push_credentials(&self) -> Result<(), String> {
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
        if result.exit_code == 0 {
            Ok(())
        } else {
            Err(format!(
                "Failed to refresh push credentials (exit {}): {}",
                result.exit_code, result.stderr
            ))
        }
    }

    async fn setup_git_for_run(&self, run_id: &str) -> Result<Option<crate::GitRunInfo>, String> {
        setup_git_via_exec(self, run_id).await.map(Some)
    }

    fn resume_setup_commands(&self, run_branch: &str) -> Vec<String> {
        vec![format!(
            "git fetch origin {run_branch} && git checkout {run_branch}"
        )]
    }

    async fn git_push_branch(&self, branch: &str) -> bool {
        git_push_via_exec(self, branch).await
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
        .and_then(|ip_address| ip_address.fqdn.clone().or_else(|| ip_address.ip.clone()))
        .ok_or_else(|| "container group has no reachable IP address yet".to_string())?;
    Ok(format!("http://{host}:{sandboxd_port}"))
}
