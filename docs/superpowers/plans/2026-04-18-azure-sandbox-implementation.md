# Azure Sandbox Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add an Azure-backed sandbox provider that lets Fabro run `fabro-server` as a singleton control plane in Azure Container Apps and create preserved workflow sandboxes in Azure Container Instances.

**Architecture:** Introduce `AzureSandbox` in `fabro-sandbox`, backed by Azure ARM REST for lifecycle and a small in-sandbox `sandboxd` HTTP daemon for `exec` and file operations. Wire Azure config through `fabro-types`, `fabro-workflow`, `fabro-cli`, and `fabro-server`, preserving Daytona-style remote git and reconnect semantics while using Docker-style image-backed container execution.

**Tech Stack:** Rust, Axum, Reqwest, Tokio, Azure ARM REST, Azure Container Instances, Azure Container Registry, Azure Files, Cargo Nextest

---

## File Structure

### New Files

- `lib/crates/fabro-sandbox/src/azure/mod.rs`
  - `AzureSandbox` implementation and `Sandbox` trait methods
- `lib/crates/fabro-sandbox/src/azure/config.rs`
  - Azure platform config loaded from env plus run-level Azure runtime config
- `lib/crates/fabro-sandbox/src/azure/arm.rs`
  - Azure ARM REST client for container-group create/get/delete
- `lib/crates/fabro-sandbox/src/azure/resource_id.rs`
  - Parsing and formatting helpers for ACI resource IDs
- `lib/crates/fabro-sandbox/src/azure/protocol.rs`
  - shared request/response types for `sandboxd`
- `lib/crates/fabro-sandbox/src/azure/sandboxd_client.rs`
  - client used by `AzureSandbox` to call the in-sandbox daemon
- `lib/crates/fabro-sandbox/src/repo.rs`
  - provider-neutral repo detection helpers moved out of Daytona
- `lib/crates/fabro-sandboxd/Cargo.toml`
  - new crate manifest for the in-sandbox daemon
- `lib/crates/fabro-sandboxd/src/lib.rs`
  - Axum router and handlers for `sandboxd`
- `lib/crates/fabro-sandboxd/src/main.rs`
  - daemon binary entry point
- `lib/crates/fabro-sandboxd/tests/http.rs`
  - protocol tests for health, exec, and file routes
- `lib/crates/fabro-sandbox/tests/azure_provider.rs`
  - mocked provider tests for lifecycle, reconnect, and sandboxd transport
- `lib/crates/fabro-workflow/tests/it/azure_integration.rs`
  - ignored Azure live tests for create/exec/reconnect
- `docs/administration/azure-hosting.md`
  - operator docs for ACA singleton deployment and Azure env vars

### Modified Files

- `Cargo.toml`
  - workspace dependencies for any new shared crates/deps
- `lib/crates/fabro-types/src/settings/run.rs`
  - add `[run.sandbox.azure]` layer and resolved settings
- `lib/crates/fabro-sandbox/Cargo.toml`
  - add Azure feature/dependencies and `httpmock` dev-dependency
- `lib/crates/fabro-sandbox/src/lib.rs`
  - export Azure module
- `lib/crates/fabro-sandbox/src/config.rs`
  - runtime `AzureConfig`
- `lib/crates/fabro-sandbox/src/sandbox_provider.rs`
  - add `SandboxProvider::Azure`
- `lib/crates/fabro-sandbox/src/sandbox_spec.rs`
  - add `SandboxSpec::Azure`, provider name, build path, and record handling
- `lib/crates/fabro-sandbox/src/reconnect.rs`
  - reconnect saved Azure sandboxes
- `lib/crates/fabro-sandbox/src/daytona/mod.rs`
  - switch repo detection to the new shared helper
- `lib/crates/fabro-workflow/Cargo.toml`
  - enable `fabro-sandbox` Azure feature
- `lib/crates/fabro-workflow/src/operations/start.rs`
  - resolve Azure config and build `SandboxSpec::Azure`
- `lib/crates/fabro-cli/Cargo.toml`
  - enable `fabro-sandbox` Azure feature
- `lib/crates/fabro-cli/src/commands/run/runner.rs`
  - request GitHub credentials for Azure sandboxes too
- `lib/crates/fabro-server/Cargo.toml`
  - enable `fabro-sandbox` Azure feature
- `lib/crates/fabro-server/src/run_manifest.rs`
  - Azure preflight validation and sandbox creation checks
- `lib/crates/fabro-workflow/tests/it/main.rs`
  - register Azure integration tests

## Environment Contract

The first branch should keep Azure deployment settings out of run TOML and load them from environment in the control plane.

Required env vars for `fabro-server` / Azure provider:

- `FABRO_AZURE_SUBSCRIPTION_ID`
- `FABRO_AZURE_RESOURCE_GROUP`
- `FABRO_AZURE_LOCATION`
- `FABRO_AZURE_SANDBOX_SUBNET_ID`
- `FABRO_AZURE_STORAGE_ACCOUNT`
- `FABRO_AZURE_STORAGE_SHARE`
- `FABRO_AZURE_ACR_SERVER`

Optional env vars:

- `FABRO_AZURE_SANDBOXD_PORT` (default `7777`)
- `AZURE_CLIENT_ID` for user-assigned managed identity
- `FABRO_AZURE_ACR_USERNAME`
- `FABRO_AZURE_ACR_PASSWORD`

Add these dev-dependencies to `lib/crates/fabro-sandbox/Cargo.toml` for the new Azure tests:

- `httpmock = "0.8"`
- `temp-env = "0.3"`

Run-level TOML for the first branch stays intentionally small:

```toml
[run.sandbox]
provider = "azure"
preserve = true

[run.sandbox.azure]
image = "fabro.azurecr.io/fabro-sandboxes/base:latest"
cpu = 2.0
memory_gb = 4.0
```

## Task 1: Add Azure Config And Provider Plumbing

**Files:**
- Modify: `Cargo.toml`
- Modify: `lib/crates/fabro-types/src/settings/run.rs`
- Modify: `lib/crates/fabro-sandbox/Cargo.toml`
- Modify: `lib/crates/fabro-sandbox/src/lib.rs`
- Modify: `lib/crates/fabro-sandbox/src/config.rs`
- Modify: `lib/crates/fabro-sandbox/src/sandbox_provider.rs`
- Modify: `lib/crates/fabro-sandbox/src/sandbox_spec.rs`
- Test: `lib/crates/fabro-types/src/settings/run.rs`
- Test: `lib/crates/fabro-sandbox/src/config.rs`
- Test: `lib/crates/fabro-sandbox/src/sandbox_provider.rs`

- [ ] **Step 1: Write the failing provider/config tests**

Add these tests to `lib/crates/fabro-sandbox/src/sandbox_provider.rs`:

```rust
#[test]
fn sandbox_provider_from_str_supports_azure() {
    assert_eq!("azure".parse::<SandboxProvider>().unwrap(), SandboxProvider::Azure);
}

#[test]
fn sandbox_provider_display_includes_azure() {
    assert_eq!(SandboxProvider::Azure.to_string(), "azure");
}
```

Add this test to `lib/crates/fabro-types/src/settings/run.rs` near the existing sandbox-layer tests:

```rust
#[test]
fn azure_sandbox_layer_deserializes() {
    let layer: RunSandboxLayer = toml::from_str(
        r#"
        provider = "azure"

        [azure]
        image = "fabro.azurecr.io/fabro-sandboxes/base:latest"
        cpu = 2.0
        memory_gb = 4.0
        "#,
    )
    .unwrap();

    assert_eq!(layer.provider.as_deref(), Some("azure"));
    let azure = layer.azure.as_ref().unwrap();
    assert_eq!(azure.image.as_deref(), Some("fabro.azurecr.io/fabro-sandboxes/base:latest"));
    assert_eq!(azure.cpu, Some(2.0));
    assert_eq!(azure.memory_gb, Some(4.0));
}
```

Add this test to `lib/crates/fabro-sandbox/src/config.rs`:

```rust
#[test]
fn azure_config_defaults() {
    let config = AzureConfig::default();
    assert!(config.image.is_none());
    assert!(config.cpu.is_none());
    assert!(config.memory_gb.is_none());
}
```

- [ ] **Step 2: Run the targeted tests and confirm they fail**

Run:

```bash
cargo nextest run -p fabro-sandbox sandbox_provider_from_str_supports_azure sandbox_provider_display_includes_azure
cargo nextest run -p fabro-types azure_sandbox_layer_deserializes
```

Expected:

- `fabro-sandbox` tests fail because `SandboxProvider::Azure` does not exist.
- `fabro-types` test fails because `RunSandboxLayer` has no `azure` field.

- [ ] **Step 3: Add the minimal Azure config and feature plumbing**

Update `lib/crates/fabro-types/src/settings/run.rs` to add resolved and layered Azure settings:

```rust
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct AzureSettings {
    pub image:     Option<String>,
    pub cpu:       Option<f64>,
    pub memory_gb: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RunSandboxSettings {
    pub provider:     String,
    pub preserve:     bool,
    pub devcontainer: bool,
    pub env:          HashMap<String, InterpString>,
    pub local:        LocalSandboxSettings,
    pub daytona:      Option<DaytonaSettings>,
    pub azure:        Option<AzureSettings>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AzureSandboxLayer {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image:     Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu:       Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_gb: Option<f64>,
}
```

Update `lib/crates/fabro-sandbox/src/config.rs`:

```rust
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct AzureConfig {
    pub image:     Option<String>,
    pub cpu:       Option<f64>,
    pub memory_gb: Option<f64>,
}
```

Update `lib/crates/fabro-sandbox/src/sandbox_provider.rs`:

```rust
pub enum SandboxProvider {
    Local,
    Docker,
    Daytona,
    Azure,
}
```

Update `lib/crates/fabro-sandbox/src/sandbox_spec.rs`:

```rust
#[cfg(feature = "azure")]
use crate::azure::AzureSandbox;
#[cfg(feature = "azure")]
use crate::config::AzureConfig;

pub enum SandboxSpec {
    Local { working_directory: PathBuf },
    #[cfg(feature = "docker")]
    Docker { config: DockerSandboxOptions },
    #[cfg(feature = "daytona")]
    Daytona {
        config:       DaytonaConfig,
        github_app:   Option<GitHubCredentials>,
        run_id:       Option<RunId>,
        clone_branch: Option<String>,
        api_key:      Option<String>,
    },
    #[cfg(feature = "azure")]
    Azure {
        config:       AzureConfig,
        github_app:   Option<GitHubCredentials>,
        run_id:       Option<RunId>,
        clone_branch: Option<String>,
    },
}
```

Update `lib/crates/fabro-sandbox/Cargo.toml` and dependent crate manifests:

```toml
[features]
default = ["local"]
azure = ["dep:reqwest", "dep:fabro-github", "dep:fabro-config"]

[dependencies]
reqwest.workspace = true
```

Update `lib/crates/fabro-sandbox/src/lib.rs`:

```rust
#[cfg(feature = "azure")]
pub mod azure;
```

Update `lib/crates/fabro-workflow/Cargo.toml`, `lib/crates/fabro-server/Cargo.toml`, and `lib/crates/fabro-cli/Cargo.toml` to enable `fabro-sandbox` with `features = ["daytona", "azure"]`.

- [ ] **Step 4: Re-run the targeted tests and a focused build**

Run:

```bash
cargo nextest run -p fabro-sandbox sandbox_provider_from_str_supports_azure sandbox_provider_display_includes_azure
cargo nextest run -p fabro-types azure_sandbox_layer_deserializes
cargo build -p fabro-sandbox -p fabro-types -p fabro-workflow -p fabro-server -p fabro-cli
```

Expected:

- all targeted tests pass
- all five crates compile with the new Azure feature wiring

- [ ] **Step 5: Commit the plumbing change**

```bash
git add Cargo.toml lib/crates/fabro-types/src/settings/run.rs lib/crates/fabro-sandbox/Cargo.toml lib/crates/fabro-sandbox/src/lib.rs lib/crates/fabro-sandbox/src/config.rs lib/crates/fabro-sandbox/src/sandbox_provider.rs lib/crates/fabro-sandbox/src/sandbox_spec.rs lib/crates/fabro-workflow/Cargo.toml lib/crates/fabro-server/Cargo.toml lib/crates/fabro-cli/Cargo.toml
git commit -m "feat: add azure sandbox provider plumbing"
```

## Task 2: Build The Shared `sandboxd` Protocol And Daemon

**Files:**
- Create: `lib/crates/fabro-sandbox/src/azure/protocol.rs`
- Create: `lib/crates/fabro-sandboxd/Cargo.toml`
- Create: `lib/crates/fabro-sandboxd/src/lib.rs`
- Create: `lib/crates/fabro-sandboxd/src/main.rs`
- Create: `lib/crates/fabro-sandboxd/tests/http.rs`
- Test: `lib/crates/fabro-sandboxd/tests/http.rs`

- [ ] **Step 1: Write the failing `sandboxd` HTTP tests**

Create `lib/crates/fabro-sandboxd/tests/http.rs`:

```rust
use axum::body::Body;
use axum::http::{Request, StatusCode};
use fabro_sandbox::azure::protocol::{ExecRequest, ExecResponse, ReadFileRequest, WriteFileRequest};
use fabro_sandboxd::build_router;
use tower::ServiceExt;

#[tokio::test]
async fn health_endpoint_returns_ok() {
    let app = build_router();
    let response = app
        .oneshot(Request::builder().uri("/health").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn exec_endpoint_runs_command() {
    let app = build_router();
    let body = serde_json::to_vec(&ExecRequest {
        command: "printf hello".to_string(),
        working_dir: None,
        env: std::collections::HashMap::new(),
        timeout_ms: 5_000,
    })
    .unwrap();
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/exec")
                .header("content-type", "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn write_then_read_round_trip() {
    let app = build_router();
    let write_body = serde_json::to_vec(&WriteFileRequest {
        path: "/tmp/sandboxd-round-trip.txt".to_string(),
        content_base64: base64::encode("hello azure"),
    })
    .unwrap();
    let write_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/write-file")
                .header("content-type", "application/json")
                .body(Body::from(write_body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(write_response.status(), StatusCode::NO_CONTENT);

    let read_body = serde_json::to_vec(&ReadFileRequest {
        path: "/tmp/sandboxd-round-trip.txt".to_string(),
    })
    .unwrap();
    let read_response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/read-file")
                .header("content-type", "application/json")
                .body(Body::from(read_body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(read_response.status(), StatusCode::OK);
}
```

- [ ] **Step 2: Run the new tests and confirm they fail**

Run:

```bash
cargo nextest run -p fabro-sandboxd health_endpoint_returns_ok exec_endpoint_runs_command write_then_read_round_trip
```

Expected:

- compilation fails because `fabro-sandboxd` and the shared protocol module do not exist yet

- [ ] **Step 3: Implement the shared protocol and daemon**

Create `lib/crates/fabro-sandbox/src/azure/protocol.rs`:

```rust
use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecRequest {
    pub command:     String,
    pub working_dir: Option<String>,
    pub env:         HashMap<String, String>,
    pub timeout_ms:  u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecResponse {
    pub stdout:      String,
    pub stderr:      String,
    pub exit_code:   i32,
    pub timed_out:   bool,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadFileRequest {
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadFileResponse {
    pub content_base64: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WriteFileRequest {
    pub path:           String,
    pub content_base64: String,
}
```

Create `lib/crates/fabro-sandboxd/src/lib.rs` with a reusable router:

```rust
pub fn build_router() -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/exec", post(exec))
        .route("/read-file", post(read_file))
        .route("/write-file", post(write_file))
}
```

Create `lib/crates/fabro-sandboxd/Cargo.toml`:

```toml
[package]
name = "fabro-sandboxd"
edition.workspace = true
version.workspace = true
publish = false
license.workspace = true

[lints]
workspace = true

[dependencies]
anyhow.workspace = true
axum.workspace = true
base64.workspace = true
serde.workspace = true
serde_json.workspace = true
tokio.workspace = true
fabro-sandbox = { path = "../fabro-sandbox", features = ["azure"] }

[dev-dependencies]
tower = "0.5"
```

Implement the handlers with `tokio::process::Command` for `bash -lc`, base64 file payloads, and standard Axum JSON responses. Keep `main.rs` tiny:

```rust
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::UNSPECIFIED, 7777)).await?;
    axum::serve(listener, fabro_sandboxd::build_router()).await?;
    Ok(())
}
```

- [ ] **Step 4: Re-run the daemon tests**

Run:

```bash
cargo nextest run -p fabro-sandboxd health_endpoint_returns_ok exec_endpoint_runs_command write_then_read_round_trip
```

Expected:

- all three tests pass

- [ ] **Step 5: Commit the daemon layer**

```bash
git add lib/crates/fabro-sandbox/src/azure/protocol.rs lib/crates/fabro-sandboxd/Cargo.toml lib/crates/fabro-sandboxd/src/lib.rs lib/crates/fabro-sandboxd/src/main.rs lib/crates/fabro-sandboxd/tests/http.rs
git commit -m "feat: add sandboxd control daemon"
```

## Task 3: Add Azure Platform Config, Resource IDs, And ARM Client

**Files:**
- Create: `lib/crates/fabro-sandbox/src/azure/config.rs`
- Create: `lib/crates/fabro-sandbox/src/azure/resource_id.rs`
- Create: `lib/crates/fabro-sandbox/src/azure/arm.rs`
- Modify: `lib/crates/fabro-sandbox/Cargo.toml`
- Test: `lib/crates/fabro-sandbox/src/azure/config.rs`
- Test: `lib/crates/fabro-sandbox/src/azure/resource_id.rs`
- Test: `lib/crates/fabro-sandbox/src/azure/arm.rs`

- [ ] **Step 1: Write the failing config and resource-ID tests**

Add to `lib/crates/fabro-sandbox/src/azure/config.rs`:

```rust
#[test]
fn azure_platform_config_requires_core_env() {
    temp_env::with_vars(
        vec![
            ("FABRO_AZURE_SUBSCRIPTION_ID", None),
            ("FABRO_AZURE_RESOURCE_GROUP", None),
            ("FABRO_AZURE_LOCATION", None),
            ("FABRO_AZURE_SANDBOX_SUBNET_ID", None),
            ("FABRO_AZURE_STORAGE_ACCOUNT", None),
            ("FABRO_AZURE_STORAGE_SHARE", None),
            ("FABRO_AZURE_ACR_SERVER", None),
        ],
        || {
            let err = AzurePlatformConfig::from_env().unwrap_err();
            assert!(err.contains("FABRO_AZURE_SUBSCRIPTION_ID"));
        },
    );
}
```

Add to `lib/crates/fabro-sandbox/src/azure/resource_id.rs`:

```rust
#[test]
fn parse_container_group_resource_id_round_trips() {
    let id = "/subscriptions/sub-1/resourceGroups/rg-1/providers/Microsoft.ContainerInstance/containerGroups/fabro-run-1";
    let parsed = ContainerGroupResourceId::parse(id).unwrap();
    assert_eq!(parsed.subscription_id, "sub-1");
    assert_eq!(parsed.resource_group, "rg-1");
    assert_eq!(parsed.container_group_name, "fabro-run-1");
    assert_eq!(parsed.to_string(), id);
}
```

- [ ] **Step 2: Run the targeted tests and confirm they fail**

Run:

```bash
cargo nextest run -p fabro-sandbox azure_platform_config_requires_core_env parse_container_group_resource_id_round_trips
```

Expected:

- compilation fails because the Azure config and resource-ID helpers do not exist yet

- [ ] **Step 3: Implement env loading, MSI token acquisition, and ARM request helpers**

Create `lib/crates/fabro-sandbox/src/azure/config.rs`:

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AzurePlatformConfig {
    pub subscription_id: String,
    pub resource_group:  String,
    pub location:        String,
    pub subnet_id:       String,
    pub storage_account: String,
    pub storage_share:   String,
    pub acr_server:      String,
    pub sandboxd_port:   u16,
    pub acr_username:    Option<String>,
    pub acr_password:    Option<String>,
}
```

Implement `AzurePlatformConfig::from_env()` to read the environment contract listed at the top of this plan.

Create `lib/crates/fabro-sandbox/src/azure/resource_id.rs`:

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContainerGroupResourceId {
    pub subscription_id:     String,
    pub resource_group:      String,
    pub container_group_name: String,
}
```

Create `lib/crates/fabro-sandbox/src/azure/arm.rs` with a small REST client:

```rust
pub struct AzureArmClient {
    http:   reqwest::Client,
    config: AzurePlatformConfig,
}

impl AzureArmClient {
    pub async fn create_container_group(&self, name: &str, image: &str, cpu: f64, memory_gb: f64) -> Result<ContainerGroupResourceId, String> {
        let id = ContainerGroupResourceId::new(
            self.config.subscription_id.clone(),
            self.config.resource_group.clone(),
            name.to_string(),
        );
        let body = build_container_group_body(&self.config, name, image, cpu, memory_gb);
        self.put_json(id.arm_url(), &body).await?;
        Ok(id)
    }

    pub async fn get_container_group(&self, id: &ContainerGroupResourceId) -> Result<ContainerGroupView, String> {
        self.get_json(id.arm_url()).await
    }

    pub async fn delete_container_group(&self, id: &ContainerGroupResourceId) -> Result<(), String> {
        self.delete(id.arm_url()).await
    }
}
```

Acquire Azure bearer tokens through the managed-identity endpoint instead of adding a large Azure SDK surface in the first branch.

- [ ] **Step 4: Re-run the targeted tests and compile the sandbox crate**

Run:

```bash
cargo nextest run -p fabro-sandbox azure_platform_config_requires_core_env parse_container_group_resource_id_round_trips
cargo build -p fabro-sandbox
```

Expected:

- targeted tests pass
- `fabro-sandbox` compiles with the Azure helpers present

- [ ] **Step 5: Commit the Azure control-plane foundation**

```bash
git add lib/crates/fabro-sandbox/Cargo.toml lib/crates/fabro-sandbox/src/azure/config.rs lib/crates/fabro-sandbox/src/azure/resource_id.rs lib/crates/fabro-sandbox/src/azure/arm.rs
git commit -m "feat: add azure sandbox control-plane helpers"
```

## Task 4: Implement `AzureSandbox` Lifecycle, Reconnect, Exec, And File Ops

**Files:**
- Create: `lib/crates/fabro-sandbox/src/azure/mod.rs`
- Create: `lib/crates/fabro-sandbox/src/azure/sandboxd_client.rs`
- Create: `lib/crates/fabro-sandbox/src/repo.rs`
- Modify: `lib/crates/fabro-sandbox/src/reconnect.rs`
- Modify: `lib/crates/fabro-sandbox/src/daytona/mod.rs`
- Modify: `lib/crates/fabro-sandbox/src/sandbox_spec.rs`
- Test: `lib/crates/fabro-sandbox/tests/azure_provider.rs`

- [ ] **Step 1: Write the failing Azure provider tests**

Create `lib/crates/fabro-sandbox/tests/azure_provider.rs`:

```rust
use std::collections::HashMap;

use fabro_sandbox::config::AzureConfig;
use fabro_sandbox::{Sandbox, SandboxSpec};

fn configure_test_azure_env() {
    std::env::set_var("FABRO_AZURE_SUBSCRIPTION_ID", "sub-1");
    std::env::set_var("FABRO_AZURE_RESOURCE_GROUP", "rg-1");
    std::env::set_var("FABRO_AZURE_LOCATION", "eastus");
    std::env::set_var("FABRO_AZURE_SANDBOX_SUBNET_ID", "/subscriptions/sub-1/resourceGroups/rg-1/providers/Microsoft.Network/virtualNetworks/vnet-1/subnets/aci");
    std::env::set_var("FABRO_AZURE_STORAGE_ACCOUNT", "stor1");
    std::env::set_var("FABRO_AZURE_STORAGE_SHARE", "workspace");
    std::env::set_var("FABRO_AZURE_ACR_SERVER", "fabro.azurecr.io");
}

#[tokio::test]
async fn azure_parallel_worktree_path_uses_workspace_scratch_dir() {
    configure_test_azure_env();
    let spec = SandboxSpec::Azure {
        config: AzureConfig {
            image: Some("fabro.azurecr.io/fabro-sandboxes/base:latest".into()),
            cpu: Some(2.0),
            memory_gb: Some(4.0),
        },
        github_app: None,
        run_id: None,
        clone_branch: None,
    };
    let sandbox = spec.build(None).await.unwrap();
    let path = sandbox.parallel_worktree_path(std::path::Path::new("/tmp/run"), "run-1", "node-a", "left");
    assert_eq!(path, "/workspace/.fabro/scratch/run-1/parallel/node-a/left");
}

#[tokio::test]
async fn azure_reconnect_uses_saved_resource_id() {
    configure_test_azure_env();
    let record = fabro_sandbox::SandboxRecord {
        provider: "azure".to_string(),
        working_directory: "/workspace".to_string(),
        identifier: Some("/subscriptions/sub-1/resourceGroups/rg-1/providers/Microsoft.ContainerInstance/containerGroups/fabro-run-1".to_string()),
        host_working_directory: None,
        container_mount_point: None,
    };
    let sandbox = fabro_sandbox::reconnect::reconnect(&record, None).await.unwrap();
    assert_eq!(sandbox.working_directory(), "/workspace");
}
```

- [ ] **Step 2: Run the provider tests and confirm they fail**

Run:

```bash
cargo nextest run -p fabro-sandbox azure_parallel_worktree_path_uses_workspace_scratch_dir azure_reconnect_uses_saved_resource_id
```

Expected:

- compilation fails because `AzureSandbox` has not been implemented

- [ ] **Step 3: Implement the provider and shared repo helpers**

Create `lib/crates/fabro-sandbox/src/repo.rs` and move provider-neutral repo detection there:

```rust
pub fn detect_repo_info(path: &std::path::Path) -> Result<(String, Option<String>), String> {
    let repo = git2::Repository::discover(path).map_err(|e| e.to_string())?;
    let remote = repo.find_remote("origin").map_err(|e| e.to_string())?;
    let url = remote.url().ok_or_else(|| "origin remote has no URL".to_string())?.to_string();
    let branch = repo.head().ok().and_then(|head| head.shorthand().map(ToString::to_string));
    Ok((url, branch))
}
```

Create `lib/crates/fabro-sandbox/src/azure/sandboxd_client.rs`:

```rust
pub struct SandboxdClient {
    http:     reqwest::Client,
    base_url: String,
}

impl SandboxdClient {
    pub async fn exec(&self, request: ExecRequest) -> Result<ExecResponse, String> {
        self.http
            .post(format!("{}/exec", self.base_url))
            .json(&request)
            .send()
            .await
            .map_err(|e| e.to_string())?
            .error_for_status()
            .map_err(|e| e.to_string())?
            .json()
            .await
            .map_err(|e| e.to_string())
    }

    pub async fn read_file(&self, path: &str) -> Result<Vec<u8>, String> {
        let response: ReadFileResponse = self.http
            .post(format!("{}/read-file", self.base_url))
            .json(&ReadFileRequest { path: path.to_string() })
            .send()
            .await
            .map_err(|e| e.to_string())?
            .error_for_status()
            .map_err(|e| e.to_string())?
            .json()
            .await
            .map_err(|e| e.to_string())?;
        base64::engine::general_purpose::STANDARD
            .decode(response.content_base64)
            .map_err(|e| e.to_string())
    }

    pub async fn write_file(&self, path: &str, bytes: &[u8]) -> Result<(), String> {
        self.http
            .post(format!("{}/write-file", self.base_url))
            .json(&WriteFileRequest {
                path: path.to_string(),
                content_base64: base64::engine::general_purpose::STANDARD.encode(bytes),
            })
            .send()
            .await
            .map_err(|e| e.to_string())?
            .error_for_status()
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    pub async fn health(&self) -> Result<(), String> {
        self.http
            .get(format!("{}/health", self.base_url))
            .send()
            .await
            .map_err(|e| e.to_string())?
            .error_for_status()
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}
```

Create `lib/crates/fabro-sandbox/src/azure/mod.rs` with these fields:

```rust
pub struct AzureSandbox {
    runtime:        AzureConfig,
    platform:       AzurePlatformConfig,
    arm:            AzureArmClient,
    sandboxd:       tokio::sync::OnceCell<SandboxdClient>,
    resource_id:    tokio::sync::OnceCell<ContainerGroupResourceId>,
    github_app:     Option<fabro_github::GitHubCredentials>,
    run_id:         Option<fabro_types::RunId>,
    clone_branch:   Option<String>,
    origin_url:     tokio::sync::OnceCell<String>,
    event_callback: Option<SandboxEventCallback>,
}
```

Implement these methods first:

- `new()` loads `AzurePlatformConfig::from_env()`
- `initialize()` creates the ACI container group, waits for `sandboxd /health`, clones the repo into `/workspace`, and emits lifecycle events
- `cleanup()` deletes the container group
- `working_directory()` returns `/workspace`
- `parallel_worktree_path()` returns `/workspace/.fabro/scratch/<run_id>/parallel/<node>/<key>`
- `sandbox_info()` returns the full ARM resource ID string
- `exec_command()`, `read_file()`, `write_file()`, `upload_file_from_local()`, `download_file_to_local()`, `list_directory()`, and `file_exists()` call `sandboxd`
- `setup_git_for_run()`, `resume_setup_commands()`, `git_push_branch()`, and `refresh_push_credentials()` reuse the same remote-git pattern as Daytona

Update `lib/crates/fabro-sandbox/src/reconnect.rs`:

```rust
        #[cfg(feature = "azure")]
        "azure" => {
            let id = record
                .identifier
                .as_deref()
                .context("Azure sandbox record missing identifier (container group resource ID)")?;
            let sandbox = crate::azure::AzureSandbox::reconnect(id.to_string()).await?;
            Ok(Box::new(sandbox))
        }
```

Update `lib/crates/fabro-sandbox/src/daytona/mod.rs` to import `crate::repo::detect_repo_info` instead of keeping repo detection local.

- [ ] **Step 4: Re-run provider tests and targeted sandbox tests**

Run:

```bash
cargo nextest run -p fabro-sandbox azure_parallel_worktree_path_uses_workspace_scratch_dir azure_reconnect_uses_saved_resource_id detect_git_remote_from_repo detect_git_branch_from_repo
```

Expected:

- Azure provider tests pass
- Daytona repo-detection tests still pass through the shared helper

- [ ] **Step 5: Commit the Azure provider implementation**

```bash
git add lib/crates/fabro-sandbox/src/azure/mod.rs lib/crates/fabro-sandbox/src/azure/sandboxd_client.rs lib/crates/fabro-sandbox/src/repo.rs lib/crates/fabro-sandbox/src/reconnect.rs lib/crates/fabro-sandbox/src/daytona/mod.rs lib/crates/fabro-sandbox/src/sandbox_spec.rs lib/crates/fabro-sandbox/tests/azure_provider.rs
git commit -m "feat: add azure sandbox provider"
```

## Task 5: Wire Azure Through Workflow, CLI, And Server Preflight

**Files:**
- Modify: `lib/crates/fabro-workflow/src/operations/start.rs`
- Modify: `lib/crates/fabro-cli/src/commands/run/runner.rs`
- Modify: `lib/crates/fabro-server/src/run_manifest.rs`
- Test: `lib/crates/fabro-workflow/src/operations/start.rs`
- Test: `lib/crates/fabro-cli/src/commands/run/runner.rs`
- Test: `lib/crates/fabro-server/src/run_manifest.rs`

- [ ] **Step 1: Write the failing workflow and preflight tests**

Add this test to `lib/crates/fabro-workflow/src/operations/start.rs` near the existing config-bridge tests:

```rust
#[test]
fn runtime_azure_config_preserves_image_cpu_and_memory() {
    let settings = fabro_types::settings::run::AzureSettings {
        image: Some("fabro.azurecr.io/fabro-sandboxes/base:latest".to_string()),
        cpu: Some(2.0),
        memory_gb: Some(4.0),
    };
    let runtime = runtime_azure_config(&settings);
    assert_eq!(runtime.image.as_deref(), Some("fabro.azurecr.io/fabro-sandboxes/base:latest"));
    assert_eq!(runtime.cpu, Some(2.0));
    assert_eq!(runtime.memory_gb, Some(4.0));
}
```

Add this test to `lib/crates/fabro-cli/src/commands/run/runner.rs`:

```rust
#[test]
fn maybe_build_github_credentials_requires_them_for_azure_runs() {
    let settings: fabro_types::settings::SettingsLayer = toml::from_str(
        r#"
        [run.sandbox]
        provider = "azure"

        [run.sandbox.azure]
        image = "fabro.azurecr.io/fabro-sandboxes/base:latest"
        "#,
    )
    .unwrap();

    let result = maybe_build_github_credentials(&settings, None);
    assert!(result.is_err());
}
```

Add this test to `lib/crates/fabro-server/src/run_manifest.rs`:

```rust
#[tokio::test]
async fn preflight_azure_without_platform_env_returns_report() {
    let settings: fabro_types::settings::SettingsLayer = toml::from_str(
        r#"
        [run.sandbox]
        provider = "azure"

        [run.sandbox.azure]
        image = "fabro.azurecr.io/fabro-sandboxes/base:latest"
        "#,
    )
    .unwrap();

    let (report, ok) = preflight_for_settings(settings).await.unwrap();
    assert!(!ok);
    assert!(report.sections[0]
        .checks
        .iter()
        .any(|check| check.remediation.as_deref().is_some_and(|text| text.contains("FABRO_AZURE_SUBSCRIPTION_ID"))));
}
```

- [ ] **Step 2: Run the targeted tests and confirm they fail**

Run:

```bash
cargo nextest run -p fabro-workflow runtime_azure_config_preserves_image_cpu_and_memory
cargo nextest run -p fabro-cli maybe_build_github_credentials_requires_them_for_azure_runs
cargo nextest run -p fabro-server preflight_azure_without_platform_env_returns_report
```

Expected:

- workflow test fails because `runtime_azure_config()` does not exist
- CLI test fails because Azure is not treated like Daytona for GitHub credentials
- server test fails because Azure preflight does not exist

- [ ] **Step 3: Implement the wiring**

Update `lib/crates/fabro-workflow/src/operations/start.rs`:

```rust
            SandboxProvider::Azure => SandboxSpec::Azure {
                config: resolve_azure_config(&resolved).unwrap_or_default(),
                github_app: services.github_app.clone(),
                run_id: Some(record.run_id),
                clone_branch: detected_base_branch.or_else(|| record.base_branch.clone()),
            },
```

Add the bridge helper:

```rust
fn resolve_azure_config(settings: &ResolvedRunSettings) -> Option<AzureConfig> {
    settings.sandbox.azure.as_ref().map(runtime_azure_config)
}

fn runtime_azure_config(settings: &fabro_types::settings::run::AzureSettings) -> fabro_sandbox::config::AzureConfig {
    fabro_sandbox::config::AzureConfig {
        image: settings.image.clone(),
        cpu: settings.cpu,
        memory_gb: settings.memory_gb,
    }
}
```

Update `lib/crates/fabro-cli/src/commands/run/runner.rs`:

```rust
    let required_github_credentials = resolved_run.as_ref().is_some_and(|settings| {
        settings.execution.mode != RunMode::DryRun
            && matches!(settings.sandbox.provider.as_str(), "daytona" | "azure")
    }) || resolved_server
        .as_ref()
        .is_some_and(|settings| !settings.integrations.github.permissions.is_empty());
```

Update `lib/crates/fabro-server/src/run_manifest.rs` to treat Azure as a preflighted cloud provider and validate `AzurePlatformConfig::from_env()` before attempting sandbox creation.

- [ ] **Step 4: Re-run the targeted tests and a focused multi-crate build**

Run:

```bash
cargo nextest run -p fabro-workflow runtime_azure_config_preserves_image_cpu_and_memory
cargo nextest run -p fabro-cli maybe_build_github_credentials_requires_them_for_azure_runs
cargo nextest run -p fabro-server preflight_azure_without_platform_env_returns_report
cargo build -p fabro-workflow -p fabro-cli -p fabro-server
```

Expected:

- all targeted tests pass
- all three crates compile with Azure provider wiring

- [ ] **Step 5: Commit the workflow/server/CLI wiring**

```bash
git add lib/crates/fabro-workflow/src/operations/start.rs lib/crates/fabro-cli/src/commands/run/runner.rs lib/crates/fabro-server/src/run_manifest.rs
git commit -m "feat: wire azure sandboxes through workflow and server"
```

## Task 6: Add Live Azure Tests And Operator Documentation

**Files:**
- Create: `lib/crates/fabro-workflow/tests/it/azure_integration.rs`
- Modify: `lib/crates/fabro-workflow/tests/it/main.rs`
- Create: `docs/administration/azure-hosting.md`
- Test: `lib/crates/fabro-workflow/tests/it/azure_integration.rs`

- [ ] **Step 1: Write the ignored Azure live tests first**

Create `lib/crates/fabro-workflow/tests/it/azure_integration.rs`:

```rust
#![allow(clippy::print_stderr)]

use fabro_agent::Sandbox;
use fabro_sandbox::config::AzureConfig;
use fabro_sandbox::SandboxSpec;

#[tokio::test]
#[ignore = "requires Azure credentials and network access"]
async fn azure_exec_command_round_trip() {
    let sandbox = SandboxSpec::Azure {
        config: AzureConfig {
            image: Some(std::env::var("FABRO_AZURE_TEST_IMAGE").unwrap()),
            cpu: Some(2.0),
            memory_gb: Some(4.0),
        },
        github_app: None,
        run_id: None,
        clone_branch: None,
    }
    .build(None)
    .await
    .unwrap();

    sandbox.initialize().await.unwrap();
    let result = sandbox.exec_command("printf hello", 10_000, None, None, None).await.unwrap();
    assert_eq!(result.stdout, "hello");
    sandbox.cleanup().await.unwrap();
}
```

Register the file in `lib/crates/fabro-workflow/tests/it/main.rs`:

```rust
mod azure_integration;
```

- [ ] **Step 2: Run the ignored-test file in compile mode**

Run:

```bash
cargo nextest run -p fabro-workflow --no-run azure_exec_command_round_trip
```

Expected:

- test binary builds successfully
- the ignored Azure live test is listed but not executed

- [ ] **Step 3: Write the operator doc for ACA singleton deployment**

Create `docs/administration/azure-hosting.md` with these sections:

```md
# Azure Hosting

## Control Plane

Deploy `fabro-server` to Azure Container Apps with:

- exactly one active replica
- scale-to-zero disabled
- managed identity enabled

## Required Environment Variables

- `FABRO_AZURE_SUBSCRIPTION_ID`
- `FABRO_AZURE_RESOURCE_GROUP`
- `FABRO_AZURE_LOCATION`
- `FABRO_AZURE_SANDBOX_SUBNET_ID`
- `FABRO_AZURE_STORAGE_ACCOUNT`
- `FABRO_AZURE_STORAGE_SHARE`
- `FABRO_AZURE_ACR_SERVER`

## Sandbox Runtime

Workflow sandboxes run as Azure Container Instances with `/workspace` mounted from Azure Files.
```

Keep the doc explicit about the singleton requirement for the first branch.

- [ ] **Step 4: Run the relevant tests and a docs sanity build**

Run:

```bash
cargo nextest run -p fabro-workflow --no-run azure_exec_command_round_trip
cargo build -p fabro-sandboxd -p fabro-sandbox -p fabro-workflow -p fabro-server -p fabro-cli
```

Expected:

- Azure live tests compile
- the new daemon crate and Azure-enabled crates all build together

- [ ] **Step 5: Commit tests and docs**

```bash
git add lib/crates/fabro-workflow/tests/it/azure_integration.rs lib/crates/fabro-workflow/tests/it/main.rs docs/administration/azure-hosting.md
git commit -m "test: add azure sandbox integration coverage"
```

## Final Verification

- [ ] **Step 1: Run the focused Azure-related test set**

```bash
cargo nextest run -p fabro-sandbox azure_platform_config_requires_core_env parse_container_group_resource_id_round_trips azure_parallel_worktree_path_uses_workspace_scratch_dir azure_reconnect_uses_saved_resource_id
cargo nextest run -p fabro-sandboxd health_endpoint_returns_ok exec_endpoint_runs_command write_then_read_round_trip
cargo nextest run -p fabro-workflow runtime_azure_config_preserves_image_cpu_and_memory
cargo nextest run -p fabro-cli maybe_build_github_credentials_requires_them_for_azure_runs
cargo nextest run -p fabro-server preflight_azure_without_platform_env_returns_report
```

Expected:

- all targeted Azure unit/integration tests pass locally without cloud access

- [ ] **Step 2: Run the main compile/test safety net**

```bash
cargo build --workspace
ulimit -n 4096 && cargo nextest run --workspace
```

Expected:

- workspace builds cleanly
- full test suite passes

- [ ] **Step 3: Run formatting and linting**

```bash
cargo +nightly-2026-04-14 fmt --all
cargo +nightly-2026-04-14 fmt --check --all
cargo +nightly-2026-04-14 clippy --workspace --all-targets -- -D warnings
```

Expected:

- no formatting diffs remain
- clippy passes with no warnings

- [ ] **Step 4: Run the ignored Azure live test when credentials are available**

```bash
cargo nextest run -p fabro-workflow --run-ignored ignored azure_exec_command_round_trip
```

Expected:

- the test provisions a sandbox, runs `printf hello`, and cleans up successfully

- [ ] **Step 5: Final commit if verification required follow-up edits**

```bash
git add .
git commit -m "feat: add azure sandbox runtime"
```
