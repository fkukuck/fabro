# Azure Platform Config Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move Azure sandbox platform setup into `fabro install`, persist non-secret Azure platform settings in structured server config, persist ACR auth in the vault, and make Azure runtime consume a server-written snapshot file instead of `FABRO_AZURE_*` worker env.

**Architecture:** Add a first-class `[server.sandbox.azure.platform]` config surface, extend the install API and UI with an Azure step, and resolve Azure platform config on the server from structured settings plus vault secrets. Write the resolved config to a runtime snapshot file and make preflight, worker sandbox startup, and reconnect load that snapshot instead of rediscovering Azure config from process env.

**Tech Stack:** Rust (`axum`, `serde`, `toml`, `nextest`), TypeScript/React (`bun`, React Router), OpenAPI (`docs/api-reference/fabro-api.yaml`, generated Rust and TS clients)

---

## File Map

- `docs/api-reference/fabro-api.yaml`
  The source-of-truth contract for browser-install Azure session, request, and summary shapes.

- `lib/crates/fabro-types/src/settings/server.rs`
  Resolved server settings types. Add the structured Azure platform config surface under `[server.sandbox.azure.platform]`.

- `lib/crates/fabro-config/src/layers/server.rs`
  Sparse settings-layer parsing. Add the matching optional install-persisted Azure layer.

- `lib/crates/fabro-config/src/resolve/server.rs`
  Resolve sparse server layer into the concrete `ServerNamespace` value.

- `lib/crates/fabro-config/src/tests/resolve_server.rs`
  Prove the new `[server.sandbox.azure.platform]` settings resolve correctly.

- `lib/crates/fabro-static/src/env_vars.rs`
  Declare the fixed names for vault-stored Azure ACR secrets.

- `lib/crates/fabro-install/src/lib.rs`
  Shared install persistence helpers that write structured Azure platform settings into `settings.toml`.

- `lib/crates/fabro-server/src/install.rs`
  Browser-install state machine, Azure step endpoint, session redaction, and install-finish persistence.

- `lib/crates/fabro-server/tests/it/api/install.rs`
  End-to-end install API tests for Azure redaction and persistence.

- `apps/fabro-web/app/install-api.ts`
  Browser install request helpers for the new Azure step.

- `apps/fabro-web/app/install-app.tsx`
  Browser install wizard stepper, Azure form, validation, review summary, and navigation order.

- `apps/fabro-web/app/install-app.test.tsx`
  UI tests for the Azure step and review rendering.

- `apps/fabro-web/app/install-api.test.ts`
  Request helper tests for the `/install/azure` request.

- `lib/packages/fabro-api-client/src/models/install-azure-config-input.ts`
  Generated TS request type for the Azure install step.

- `lib/packages/fabro-api-client/src/models/install-azure-summary.ts`
  Generated TS redacted session summary type for Azure install state.

- `lib/packages/fabro-api-client/src/models/install-session-response.ts`
  Generated TS install session type extended with `azure`.

- `lib/packages/fabro-api-client/src/models/index.ts`
  Generated TS exports for the new Azure install models.

- `lib/crates/fabro-config/src/storage.rs`
  Runtime-directory path helper for the Azure platform snapshot file.

- `lib/crates/fabro-server/src/azure_platform.rs`
  New server-owned Azure platform resolver and runtime snapshot writer.

- `lib/crates/fabro-server/src/server.rs`
  Build app state, write the Azure platform snapshot, and explicitly set `FABRO_STORAGE_ROOT` for workers.

- `lib/crates/fabro-server/src/run_manifest.rs`
  Replace Azure preflight env validation with the server-owned resolver.

- `lib/crates/fabro-server/src/run_files.rs`
  Pass storage-root context into Azure reconnect.

- `lib/crates/fabro-sandbox/src/azure/config.rs`
  Runtime `AzurePlatformConfig`, serde support, and snapshot-file load helpers.

- `lib/crates/fabro-sandbox/src/azure/mod.rs`
  Azure sandbox startup/reconnect paths stop calling `from_env()` and load the runtime snapshot instead.

- `lib/crates/fabro-sandbox/src/reconnect.rs`
  Reconnect API grows the storage-root input needed to load the Azure snapshot outside worker mode.

- `lib/crates/fabro-sandbox/tests/azure_provider.rs`
  Snapshot-based Azure provider tests replacing direct `FABRO_AZURE_*` process env setup.

### Task 1: Add Structured Azure Platform Settings To Server Config

**Files:**
- Modify: `lib/crates/fabro-types/src/settings/server.rs`
- Modify: `lib/crates/fabro-config/src/layers/server.rs`
- Modify: `lib/crates/fabro-config/src/resolve/server.rs`
- Modify: `lib/crates/fabro-config/src/tests/resolve_server.rs`
- Modify: `lib/crates/fabro-static/src/env_vars.rs`

- [ ] **Step 1: Write the failing config-resolution tests**

```rust
#[test]
fn resolve_server_keeps_structured_azure_platform_settings() {
    let settings = ServerSettingsBuilder::from_toml(
        r#"
_version = 1

[server.auth]
methods = ["dev-token"]

[server.sandbox.azure.platform]
subscription_id = "sub-1"
resource_group = "rg-1"
location = "eastus"
subnet_id = "/subscriptions/sub-1/resourceGroups/rg-1/providers/Microsoft.Network/virtualNetworks/vnet-1/subnets/aci"
acr_server = "fabro.azurecr.io"
sandboxd_port = 7777
"#,
    )
    .unwrap()
    .server;

    let platform = settings
        .sandbox
        .azure
        .as_ref()
        .and_then(|azure| azure.platform.as_ref())
        .expect("azure platform settings should resolve");

    assert_eq!(platform.subscription_id, "sub-1");
    assert_eq!(platform.acr_server, "fabro.azurecr.io");
    assert_eq!(platform.sandboxd_port, 7777);
}

#[test]
fn env_var_constants_include_azure_acr_secret_names() {
    assert_eq!(EnvVars::FABRO_AZURE_ACR_USERNAME, "FABRO_AZURE_ACR_USERNAME");
    assert_eq!(EnvVars::FABRO_AZURE_ACR_PASSWORD, "FABRO_AZURE_ACR_PASSWORD");
}
```

- [ ] **Step 2: Run the config tests to verify they fail**

Run: `cargo nextest run -p fabro-config resolve_server_keeps_structured_azure_platform_settings`

Run: `cargo nextest run -p fabro-static env_var_constants_include_azure_acr_secret_names`

Expected: FAIL with unknown `server.sandbox` settings and missing Azure env-var constants.

- [ ] **Step 3: Add the structured server config types and constants**

```rust
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerSandboxSettings {
    pub azure: Option<ServerAzureSandboxSettings>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerAzureSandboxSettings {
    pub platform: Option<ServerAzurePlatformSettings>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerAzurePlatformSettings {
    pub subscription_id: String,
    pub resource_group: String,
    pub location: String,
    pub subnet_id: String,
    pub acr_server: String,
    pub sandboxd_port: u16,
}

impl Default for ServerAzurePlatformSettings {
    fn default() -> Self {
        Self {
            subscription_id: String::new(),
            resource_group: String::new(),
            location: String::new(),
            subnet_id: String::new(),
            acr_server: String::new(),
            sandboxd_port: 7777,
        }
    }
}
```

```rust
pub const FABRO_AZURE_ACR_USERNAME: &'static str = "FABRO_AZURE_ACR_USERNAME";
pub const FABRO_AZURE_ACR_PASSWORD: &'static str = "FABRO_AZURE_ACR_PASSWORD";
```

Also thread the new layer through `ServerLayer` and `resolve_server()` so `ServerNamespace` gets a populated `sandbox` field.

- [ ] **Step 4: Re-run the config tests to verify they pass**

Run: `cargo nextest run -p fabro-config resolve_server_keeps_structured_azure_platform_settings`

Run: `cargo nextest run -p fabro-static env_var_constants_include_azure_acr_secret_names`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add lib/crates/fabro-types/src/settings/server.rs lib/crates/fabro-config/src/layers/server.rs lib/crates/fabro-config/src/resolve/server.rs lib/crates/fabro-config/src/tests/resolve_server.rs lib/crates/fabro-static/src/env_vars.rs
git commit -m "feat(config): add structured azure platform settings"
```

### Task 2: Add Install Persistence Helpers For Azure Platform Settings

**Files:**
- Modify: `lib/crates/fabro-install/src/lib.rs`

- [ ] **Step 1: Write the failing persistence helper test**

```rust
#[test]
fn write_azure_platform_settings_creates_server_sandbox_platform_table() {
    let mut doc = toml::Value::Table(toml::Table::new());

    write_azure_platform_settings(
        &mut doc,
        &InstallAzurePlatformSelection {
            subscription_id: "sub-1".into(),
            resource_group: "rg-1".into(),
            location: "eastus".into(),
            subnet_id: "/subscriptions/sub-1/resourceGroups/rg-1/providers/Microsoft.Network/virtualNetworks/vnet-1/subnets/aci".into(),
            acr_server: "fabro.azurecr.io".into(),
            sandboxd_port: 7777,
        },
    )
    .unwrap();

    let rendered = toml::to_string(&doc).unwrap();
    assert!(rendered.contains("[server.sandbox.azure.platform]"));
    assert!(rendered.contains("subscription_id = \"sub-1\""));
    assert!(rendered.contains("acr_server = \"fabro.azurecr.io\""));
}
```

- [ ] **Step 2: Run the helper test to verify it fails**

Run: `cargo nextest run -p fabro-install write_azure_platform_settings_creates_server_sandbox_platform_table`

Expected: FAIL because `write_azure_platform_settings` and `InstallAzurePlatformSelection` do not exist.

- [ ] **Step 3: Add the minimal helper and selection type**

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallAzurePlatformSelection {
    pub subscription_id: String,
    pub resource_group: String,
    pub location: String,
    pub subnet_id: String,
    pub acr_server: String,
    pub sandboxd_port: u16,
}

pub fn write_azure_platform_settings(
    doc: &mut toml::Value,
    selection: &InstallAzurePlatformSelection,
) -> Result<()> {
    let root = root_table_mut(doc)?;
    let server = ensure_table(root, "server")?;
    let sandbox = ensure_table(server, "sandbox")?;
    let azure = ensure_table(sandbox, "azure")?;
    let platform = ensure_table(azure, "platform")?;

    platform.insert("subscription_id".into(), toml::Value::String(selection.subscription_id.clone()));
    platform.insert("resource_group".into(), toml::Value::String(selection.resource_group.clone()));
    platform.insert("location".into(), toml::Value::String(selection.location.clone()));
    platform.insert("subnet_id".into(), toml::Value::String(selection.subnet_id.clone()));
    platform.insert("acr_server".into(), toml::Value::String(selection.acr_server.clone()));
    platform.insert("sandboxd_port".into(), toml::Value::Integer(i64::from(selection.sandboxd_port)));
    Ok(())
}
```

- [ ] **Step 4: Re-run the helper test to verify it passes**

Run: `cargo nextest run -p fabro-install write_azure_platform_settings_creates_server_sandbox_platform_table`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add lib/crates/fabro-install/src/lib.rs
git commit -m "feat(install): add azure platform settings writer"
```

### Task 3: Extend The Install API Contract And Backend For Azure

**Files:**
- Modify: `docs/api-reference/fabro-api.yaml`
- Modify: `lib/crates/fabro-server/src/install.rs`
- Modify: `lib/crates/fabro-server/tests/it/api/install.rs`

- [ ] **Step 1: Write the failing install API tests**

```rust
#[tokio::test]
async fn install_session_redacts_saved_azure_acr_credentials() {
    let temp_dir = tempfile::tempdir().unwrap();
    let config_path = temp_dir.path().join("settings.toml");
    let app = build_install_router(InstallAppState::for_test_with_paths(
        "test-install-token",
        temp_dir.path(),
        &config_path,
    ))
    .await;

    put_install_azure(
        &app,
        "test-install-token",
        r#"{"subscription_id":"sub-1","resource_group":"rg-1","location":"eastus","subnet_id":"/subscriptions/sub-1/.../aci","acr_server":"fabro.azurecr.io","sandboxd_port":7777,"acr_username":"azure-user","acr_password":"azure-pass"}"#,
    )
    .await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/install/session")
                .header("authorization", "Bearer test-install-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = response_json(response, StatusCode::OK, "GET /install/session").await;
    assert_eq!(body["azure"]["subscription_id"], "sub-1");
    assert_eq!(body["azure"]["acr_credentials_saved"], true);
    assert!(!body.to_string().contains("azure-pass"));
}

#[tokio::test]
async fn token_install_finish_persists_azure_platform_settings_and_acr_vault_secrets() {
    let temp_dir = tempfile::tempdir().unwrap();
    let config_path = temp_dir.path().join("settings.toml");
    let app = build_install_router(InstallAppState::for_test_with_paths(
        "test-install-token",
        temp_dir.path(),
        &config_path,
    ))
    .await;

    put_install_server(&app, "test-install-token", "https://fabro.example.com").await;
    put_install_azure(
        &app,
        "test-install-token",
        r#"{"subscription_id":"sub-1","resource_group":"rg-1","location":"eastus","subnet_id":"/subscriptions/sub-1/.../aci","acr_server":"fabro.azurecr.io","sandboxd_port":7777,"acr_username":"azure-user","acr_password":"azure-pass"}"#,
    )
    .await;
    put_install_object_store(&app, "test-install-token", r#"{"provider":"local","root":"/tmp/objects"}"#).await;
    put_install_llm(&app, "test-install-token").await;
    put_install_github_token(&app, "test-install-token", "brynary").await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/install/finish")
                .header("authorization", "Bearer test-install-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    response_status(response, StatusCode::ACCEPTED, "POST /install/finish").await;

    let settings = std::fs::read_to_string(&config_path).unwrap();
    assert!(settings.contains("[server.sandbox.azure.platform]"));
    let vault = Vault::load(Storage::new(temp_dir.path()).secrets_path()).unwrap();
    assert_eq!(vault.get("FABRO_AZURE_ACR_USERNAME"), Some("azure-user"));
    assert_eq!(vault.get("FABRO_AZURE_ACR_PASSWORD"), Some("azure-pass"));
}

async fn put_install_azure(app: &Router, token: &str, body: &str) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/install/azure")
                .header("authorization", format!("Bearer {token}"))
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    response_status(response, StatusCode::NO_CONTENT, "PUT /install/azure").await;
}
```

- [ ] **Step 2: Run the install API tests to verify they fail**

Run: `cargo nextest run -p fabro-server install_session_redacts_saved_azure_acr_credentials token_install_finish_persists_azure_platform_settings_and_acr_vault_secrets`

Expected: FAIL because `/install/azure`, `azure` session output, and Azure finish persistence do not exist.

- [ ] **Step 3: Add the Azure install step, API schema, and finish persistence**

```rust
#[derive(Clone, Debug, Deserialize, Serialize)]
struct InstallAzureInput {
    subscription_id: String,
    resource_group: String,
    location: String,
    subnet_id: String,
    acr_server: String,
    sandboxd_port: Option<u16>,
    acr_username: Option<String>,
    acr_password: Option<String>,
}

async fn put_install_azure(
    State(state): State<InstallAppState>,
    headers: HeaderMap,
    Query(query): Query<InstallTokenQuery>,
    Json(mut input): Json<InstallAzureInput>,
) -> Response {
    if let Some(response) = require_valid_token(&state, &headers, query.token.as_deref()) {
        return response;
    }

    let require_field = |label: &'static str, value: String| -> Result<String, String> {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            Err(format!("{label} is required"))
        } else {
            Ok(trimmed.to_string())
        }
    };

    input.subscription_id = match require_field("subscription_id", input.subscription_id) {
        Ok(value) => value,
        Err(err) => return install_error_response(StatusCode::UNPROCESSABLE_ENTITY, err),
    };
    input.resource_group = match require_field("resource_group", input.resource_group) {
        Ok(value) => value,
        Err(err) => return install_error_response(StatusCode::UNPROCESSABLE_ENTITY, err),
    };
    input.location = match require_field("location", input.location) {
        Ok(value) => value,
        Err(err) => return install_error_response(StatusCode::UNPROCESSABLE_ENTITY, err),
    };
    input.subnet_id = match require_field("subnet_id", input.subnet_id) {
        Ok(value) => value,
        Err(err) => return install_error_response(StatusCode::UNPROCESSABLE_ENTITY, err),
    };
    input.acr_server = match require_field("acr_server", input.acr_server) {
        Ok(value) => value,
        Err(err) => return install_error_response(StatusCode::UNPROCESSABLE_ENTITY, err),
    };

    if input.acr_username.as_deref().map(str::trim).filter(|s| !s.is_empty()).is_some()
        ^ input.acr_password.as_deref().map(str::trim).filter(|s| !s.is_empty()).is_some()
    {
        return install_error_response(
            StatusCode::UNPROCESSABLE_ENTITY,
            "acr_username and acr_password must be provided together",
        );
    }

    lock_unpoisoned(&state.pending_install, "install session").azure = Some(input);
    StatusCode::NO_CONTENT.into_response()
}
```

Also update:

- `PendingInstall` to carry `azure: Option<InstallAzureInput>`
- `get_install_session()` to emit a redacted `azure` summary
- `completed_steps()` so Azure appears between `server` and `object_store`
- `post_install_finish()` to call `write_azure_platform_settings()` and push vault `Environment` secrets for `FABRO_AZURE_ACR_USERNAME` / `FABRO_AZURE_ACR_PASSWORD`
- `docs/api-reference/fabro-api.yaml` with:
  - `PUT /install/azure`
  - `InstallAzureConfigInput`
  - `InstallAzureSummary`
  - `InstallSessionResponse.azure`

- [ ] **Step 4: Re-run the install API tests to verify they pass**

Run: `cargo nextest run -p fabro-server install_session_redacts_saved_azure_acr_credentials token_install_finish_persists_azure_platform_settings_and_acr_vault_secrets`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add docs/api-reference/fabro-api.yaml lib/crates/fabro-server/src/install.rs lib/crates/fabro-server/tests/it/api/install.rs
git commit -m "feat(install): collect azure platform config"
```

### Task 4: Regenerate Install Types And Add The Azure Browser Step

**Files:**
- Modify: `lib/packages/fabro-api-client/src/models/install-session-response.ts`
- Create: `lib/packages/fabro-api-client/src/models/install-azure-config-input.ts`
- Create: `lib/packages/fabro-api-client/src/models/install-azure-summary.ts`
- Modify: `lib/packages/fabro-api-client/src/models/index.ts`
- Modify: `apps/fabro-web/app/install-api.ts`
- Modify: `apps/fabro-web/app/install-api.test.ts`
- Modify: `apps/fabro-web/app/install-app.tsx`
- Modify: `apps/fabro-web/app/install-app.test.tsx`

- [ ] **Step 1: Write the failing browser tests**

```ts
test("putInstallAzure posts the Azure config to /install/azure", async () => {
  const calls: Array<{ input: RequestInfo | URL; init?: RequestInit }> = [];
  globalThis.fetch = mock((input: RequestInfo | URL, init?: RequestInit) => {
    calls.push({ input, init });
    return Promise.resolve(new Response(null, { status: 204 }));
  }) as typeof fetch;

  await putInstallAzure("token-1", {
    subscription_id: "sub-1",
    resource_group: "rg-1",
    location: "eastus",
    subnet_id: "/subscriptions/sub-1/.../aci",
    acr_server: "fabro.azurecr.io",
    sandboxd_port: 7777,
    acr_username: "azure-user",
    acr_password: "azure-pass",
  });

  expect(String(calls[0]!.input)).toBe("/install/azure");
  expect(calls[0]!.init?.method).toBe("PUT");
});
```

```ts
test("saves Azure install settings and advances to the object store step", async () => {
  const fetchCalls: Array<{ input: RequestInfo | URL; init?: RequestInit }> = [];
  globalThis.fetch = mock((input: RequestInfo | URL, init?: RequestInit) => {
    fetchCalls.push({ input, init });
    if (String(input) === "/install/session" && fetchCalls.length === 1) {
      return Promise.resolve(
        new Response(
          JSON.stringify({
            completed_steps: ["server"],
            llm: null,
            server: { canonical_url: "https://fabro.example.com" },
            azure: null,
            object_store: null,
            github: null,
            prefill: INSTALL_PREFILL,
          }),
          { status: 200, headers: { "Content-Type": "application/json" } },
        ),
      );
    }
    if (String(input) === "/install/azure") {
      return Promise.resolve(new Response(null, { status: 204 }));
    }
    if (String(input) === "/install/session" && fetchCalls.length === 3) {
      return Promise.resolve(
        new Response(
          JSON.stringify({
            completed_steps: ["server", "azure"],
            llm: null,
            server: { canonical_url: "https://fabro.example.com" },
            azure: {
              subscription_id: "sub-1",
              resource_group: "rg-1",
              location: "eastus",
              subnet_id: "/subscriptions/sub-1/.../aci",
              acr_server: "fabro.azurecr.io",
              sandboxd_port: 7777,
              acr_credentials_saved: true,
            },
            object_store: null,
            github: null,
            prefill: INSTALL_PREFILL,
          }),
          { status: 200, headers: { "Content-Type": "application/json" } },
        ),
      );
    }
    throw new Error(`unexpected fetch: ${String(input)}`);
  }) as typeof fetch;
});
```

- [ ] **Step 2: Run the browser tests to verify they fail**

Run: `cd apps/fabro-web && bun test app/install-api.test.ts app/install-app.test.tsx`

Expected: FAIL because `putInstallAzure`, Azure session types, and the Azure wizard step do not exist.

- [ ] **Step 3: Regenerate types and implement the Azure browser step**

```ts
export async function putInstallAzure(
  token: string,
  input: InstallAzureConfigInput,
): Promise<void> {
  await installRequest(token, {
    path: "/install/azure",
    method: "PUT",
    body: input,
    errorFallback: "install azure request failed",
  });
}
```

```tsx
const INSTALL_STEPS = [
  { id: "welcome", label: "Welcome", href: "/install/welcome" },
  { id: "server", label: "Server", href: "/install/server" },
  { id: "azure", label: "Azure", href: "/install/azure" },
  { id: "object_store", label: "Object store", href: "/install/object-store" },
  { id: "llm", label: "LLMs", href: "/install/llm" },
  { id: "github", label: "GitHub", href: "/install/github" },
  { id: "review", label: "Review", href: "/install/review" },
] as const;
```

```tsx
await runStepSubmit({
  action: () =>
    putInstallAzure(installToken, {
      subscription_id: azureForm.subscriptionId.trim(),
      resource_group: azureForm.resourceGroup.trim(),
      location: azureForm.location.trim(),
      subnet_id: azureForm.subnetId.trim(),
      acr_server: azureForm.acrServer.trim(),
      sandboxd_port: azureForm.sandboxdPort.trim()
        ? Number(azureForm.sandboxdPort.trim())
        : 7777,
      acr_username: azureForm.acrUsername.trim() || undefined,
      acr_password: azureForm.acrPassword.trim() || undefined,
    }),
  fallback: "Failed to save Azure settings.",
  next: "/install/object-store",
});
```

Then regenerate the client types:

Run: `cargo build -p fabro-api`

Run: `cd lib/packages/fabro-api-client && bun run generate`

- [ ] **Step 4: Re-run the browser tests to verify they pass**

Run: `cd apps/fabro-web && bun test app/install-api.test.ts app/install-app.test.tsx`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add lib/packages/fabro-api-client/src/models/install-session-response.ts lib/packages/fabro-api-client/src/models/install-azure-config-input.ts lib/packages/fabro-api-client/src/models/install-azure-summary.ts lib/packages/fabro-api-client/src/models/index.ts apps/fabro-web/app/install-api.ts apps/fabro-web/app/install-api.test.ts apps/fabro-web/app/install-app.tsx apps/fabro-web/app/install-app.test.tsx
git commit -m "feat(web): add azure install wizard step"
```

### Task 5: Add The Server-Owned Azure Resolver And Runtime Snapshot File

**Files:**
- Modify: `lib/crates/fabro-config/src/storage.rs`
- Create: `lib/crates/fabro-server/src/azure_platform.rs`
- Modify: `lib/crates/fabro-server/src/server.rs`
- Modify: `lib/crates/fabro-server/src/run_manifest.rs`

- [ ] **Step 1: Write the failing resolver and snapshot tests**

```rust
#[test]
fn resolve_azure_platform_config_reads_settings_and_vault_secrets() {
    let settings = ServerSettingsBuilder::from_toml(
        r#"
_version = 1

[server.auth]
methods = ["dev-token"]

[server.sandbox.azure.platform]
subscription_id = "sub-1"
resource_group = "rg-1"
location = "eastus"
subnet_id = "/subscriptions/sub-1/.../aci"
acr_server = "fabro.azurecr.io"
"#,
    )
    .unwrap()
    .server;

    let resolved = resolve_azure_platform_config(&settings, &|name| match name {
        EnvVars::FABRO_AZURE_ACR_USERNAME => Some("azure-user".to_string()),
        EnvVars::FABRO_AZURE_ACR_PASSWORD => Some("azure-pass".to_string()),
        _ => None,
    })
    .unwrap()
    .expect("azure config should resolve");

    assert_eq!(resolved.subscription_id, "sub-1");
    assert_eq!(resolved.acr_username.as_deref(), Some("azure-user"));
}

#[test]
fn runtime_directory_exposes_azure_platform_snapshot_path() {
    let runtime = RuntimeDirectory::new("/srv/fabro");
    assert_eq!(runtime.azure_platform_config_path(), PathBuf::from("/srv/fabro/azure-platform.json"));
}
```

- [ ] **Step 2: Run the resolver tests to verify they fail**

Run: `cargo nextest run -p fabro-server resolve_azure_platform_config_reads_settings_and_vault_secrets`

Run: `cargo nextest run -p fabro-config runtime_directory_exposes_azure_platform_snapshot_path`

Expected: FAIL because the resolver module and snapshot path helper do not exist.

- [ ] **Step 3: Implement the resolver, snapshot path, and app-state snapshot write**

```rust
pub(crate) fn resolve_azure_platform_config(
    settings: &ServerNamespace,
    vault_lookup: &dyn Fn(&str) -> Option<String>,
) -> Result<Option<AzurePlatformConfig>, String> {
    let Some(platform) = settings
        .sandbox
        .azure
        .as_ref()
        .and_then(|azure| azure.platform.as_ref())
    else {
        return Ok(None);
    };

    let acr_username = vault_lookup(EnvVars::FABRO_AZURE_ACR_USERNAME)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let acr_password = vault_lookup(EnvVars::FABRO_AZURE_ACR_PASSWORD)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    if acr_username.is_some() ^ acr_password.is_some() {
        return Err("Azure ACR credentials must be configured together. Run fabro install to update the Azure step.".to_string());
    }

    Ok(Some(AzurePlatformConfig {
        subscription_id: platform.subscription_id.clone(),
        resource_group: platform.resource_group.clone(),
        location: platform.location.clone(),
        subnet_id: platform.subnet_id.clone(),
        acr_server: platform.acr_server.clone(),
        sandboxd_port: platform.sandboxd_port,
        acr_username,
        acr_password,
    }))
}
```

```rust
impl RuntimeDirectory {
    pub fn azure_platform_config_path(&self) -> PathBuf {
        self.root.join("azure-platform.json")
    }
}
```

```rust
fn resolved_server_storage_root(
    settings: &ResolvedAppStateSettings,
    env_lookup: &EnvLookup,
) -> anyhow::Result<PathBuf> {
    settings
        .server_settings
        .server
        .storage
        .root
        .resolve(|name| env_lookup(name))
        .map(|resolved| PathBuf::from(resolved.value))
        .map_err(anyhow::Error::from)
}

write_azure_platform_snapshot(
    &Storage::new(&resolved_server_storage_root(&resolved_settings, &env_lookup)?).runtime_directory(),
    resolve_azure_platform_config(&current_server_settings.server, &|name| {
        vault.blocking_read().get(name).map(str::to_string)
    })?,
)?;
```

Use `resolved_server_storage_root(&resolved_settings, &env_lookup)?` directly in the final code so `build_app_state()` does not depend on helpers from `serve.rs`.

Also switch `run_manifest.rs` preflight to call the resolver instead of `AzurePlatformConfig::from_env()`.

- [ ] **Step 4: Re-run the resolver tests to verify they pass**

Run: `cargo nextest run -p fabro-server resolve_azure_platform_config_reads_settings_and_vault_secrets`

Run: `cargo nextest run -p fabro-config runtime_directory_exposes_azure_platform_snapshot_path`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add lib/crates/fabro-config/src/storage.rs lib/crates/fabro-server/src/azure_platform.rs lib/crates/fabro-server/src/server.rs lib/crates/fabro-server/src/run_manifest.rs
git commit -m "feat(server): resolve and snapshot azure platform config"
```

### Task 6: Make Azure Runtime And Reconnect Consume The Snapshot File

**Files:**
- Modify: `lib/crates/fabro-sandbox/src/azure/config.rs`
- Modify: `lib/crates/fabro-sandbox/src/azure/mod.rs`
- Modify: `lib/crates/fabro-sandbox/src/reconnect.rs`
- Modify: `lib/crates/fabro-server/src/server.rs`
- Modify: `lib/crates/fabro-server/src/run_files.rs`
- Modify: `lib/crates/fabro-sandbox/tests/azure_provider.rs`

- [ ] **Step 1: Write the failing snapshot-runtime tests**

```rust
#[test]
fn azure_platform_config_loads_from_snapshot_file() {
    let dir = tempfile::tempdir().unwrap();
    let runtime = Storage::new(dir.path()).runtime_directory();
    let path = runtime.azure_platform_config_path();
    std::fs::write(
        &path,
        serde_json::to_vec(&AzurePlatformConfig {
            subscription_id: "sub-1".into(),
            resource_group: "rg-1".into(),
            location: "eastus".into(),
            subnet_id: "/subscriptions/sub-1/.../aci".into(),
            acr_server: "fabro.azurecr.io".into(),
            sandboxd_port: 7777,
            acr_username: Some("azure-user".into()),
            acr_password: Some("azure-pass".into()),
        })
        .unwrap(),
    )
    .unwrap();

    let loaded = AzurePlatformConfig::load_from_path(&path).unwrap();
    assert_eq!(loaded.subscription_id, "sub-1");
    assert_eq!(loaded.acr_username.as_deref(), Some("azure-user"));
}
```

```rust
#[tokio::test]
async fn azure_reconnect_loads_platform_snapshot_from_storage_root() {
    let dir = tempfile::tempdir().unwrap();
    let runtime = Storage::new(dir.path()).runtime_directory();
    std::fs::write(
        runtime.azure_platform_config_path(),
        serde_json::to_vec(&AzurePlatformConfig {
            subscription_id: "sub-1".into(),
            resource_group: "rg-1".into(),
            location: "eastus".into(),
            subnet_id: "/subscriptions/sub-1/.../aci".into(),
            acr_server: "fabro.azurecr.io".into(),
            sandboxd_port: 7777,
            acr_username: None,
            acr_password: None,
        })
        .unwrap(),
    )
    .unwrap();
    let record = fabro_sandbox::SandboxRecord {
        provider: "azure".to_string(),
        working_directory: "/workspace".to_string(),
        identifier: Some("/subscriptions/sub-1/resourceGroups/rg-1/providers/Microsoft.ContainerInstance/containerGroups/fabro-run-1".to_string()),
        host_working_directory: None,
        container_mount_point: None,
        repo_cloned: None,
        clone_origin_url: None,
        clone_branch: None,
    };

    let sandbox = reconnect::reconnect(&record, None, Some(dir.path())).await.unwrap();
    assert_eq!(sandbox.working_directory(), "/workspace");
}
```

- [ ] **Step 2: Run the sandbox tests to verify they fail**

Run: `cargo nextest run -p fabro-sandbox azure_platform_config_loads_from_snapshot_file azure_reconnect_loads_platform_snapshot_from_storage_root`

Expected: FAIL because Azure sandbox startup and reconnect still depend on `from_env()` and reconnect lacks storage-root input.

- [ ] **Step 3: Implement snapshot loading and reconnect plumbing**

```rust
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct AzurePlatformConfig {
    pub subscription_id: String,
    pub resource_group: String,
    pub location: String,
    pub subnet_id: String,
    pub acr_server: String,
    pub sandboxd_port: u16,
    pub acr_username: Option<String>,
    pub acr_password: Option<String>,
}

impl AzurePlatformConfig {
    pub fn load_from_path(path: &Path) -> Result<Self, String> {
        let bytes = std::fs::read(path).map_err(|err| format!("failed to read {}: {err}", path.display()))?;
        serde_json::from_slice(&bytes).map_err(|err| format!("failed to parse {}: {err}", path.display()))
    }
}
```

```rust
fn load_platform_from_worker_storage_root() -> Result<AzurePlatformConfig, String> {
    let storage_root = std::env::var(EnvVars::FABRO_STORAGE_ROOT)
        .map_err(|_| format!("{} is required to load Azure platform config", EnvVars::FABRO_STORAGE_ROOT))?;
    let path = Storage::new(storage_root).runtime_directory().azure_platform_config_path();
    AzurePlatformConfig::load_from_path(&path)
}
```

```rust
#[cfg(feature = "azure")]
"azure" => {
    let id = record
        .identifier
        .as_deref()
        .context("Azure sandbox record missing identifier (container group resource ID)")?;
    let storage_root = storage_root
        .context("Azure reconnect requires the Fabro storage root to load platform config")?;
    let sandbox = AzureSandbox::reconnect(id, storage_root)
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    Ok(Box::new(sandbox))
}
```

```rust
cmd.env(EnvVars::FABRO_STORAGE_ROOT, &storage_dir);
```

Also update server-side reconnect callsites:

```rust
reconnect(&record, daytona_api_key, Some(state.server_storage_dir().as_path())).await
```

- [ ] **Step 4: Re-run the sandbox tests to verify they pass**

Run: `cargo nextest run -p fabro-sandbox azure_platform_config_loads_from_snapshot_file azure_reconnect_loads_platform_snapshot_from_storage_root`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add lib/crates/fabro-sandbox/src/azure/config.rs lib/crates/fabro-sandbox/src/azure/mod.rs lib/crates/fabro-sandbox/src/reconnect.rs lib/crates/fabro-server/src/server.rs lib/crates/fabro-server/src/run_files.rs lib/crates/fabro-sandbox/tests/azure_provider.rs
git commit -m "fix(azure): load platform config from runtime snapshot"
```

### Task 7: Run End-To-End Verification And Refresh The Embedded SPA

**Files:**
- Modify: generated files already listed above if regeneration changes them
- Verify: `lib/crates/fabro-spa/assets/`

- [ ] **Step 1: Run the focused Rust verification suite**

```bash
cargo nextest run -p fabro-config
cargo nextest run -p fabro-install
cargo nextest run -p fabro-server
cargo nextest run -p fabro-sandbox
```

Expected: PASS for all four crates.

- [ ] **Step 2: Run the focused web verification suite**

```bash
cd apps/fabro-web && bun test
cd apps/fabro-web && bun run typecheck
```

Expected: PASS for both commands.

- [ ] **Step 3: Refresh generated API and bundled SPA assets**

```bash
cargo build -p fabro-api
cd lib/packages/fabro-api-client && bun run generate
cargo dev spa refresh
```

Expected: build succeeds, TS client regenerates cleanly, and SPA assets refresh into `lib/crates/fabro-spa/assets/`.

- [ ] **Step 4: Run formatting and a final smoke verification**

```bash
cargo +nightly-2026-04-14 fmt --all
cargo +nightly-2026-04-14 fmt --check --all
```

Expected: format completes and the check returns clean.

- [ ] **Step 5: Commit**

```bash
git add docs/api-reference/fabro-api.yaml apps/fabro-web lib/packages/fabro-api-client lib/crates/fabro-spa/assets lib/crates/fabro-types/src/settings/server.rs lib/crates/fabro-config/src/layers/server.rs lib/crates/fabro-config/src/resolve/server.rs lib/crates/fabro-config/src/tests/resolve_server.rs lib/crates/fabro-static/src/env_vars.rs lib/crates/fabro-install/src/lib.rs lib/crates/fabro-server/src/install.rs lib/crates/fabro-server/src/azure_platform.rs lib/crates/fabro-server/src/server.rs lib/crates/fabro-server/src/run_manifest.rs lib/crates/fabro-server/src/run_files.rs lib/crates/fabro-server/tests/it/api/install.rs lib/crates/fabro-sandbox/src/azure/config.rs lib/crates/fabro-sandbox/src/azure/mod.rs lib/crates/fabro-sandbox/src/reconnect.rs lib/crates/fabro-sandbox/tests/azure_provider.rs lib/crates/fabro-config/src/storage.rs
git commit -m "feat(azure): persist install-time platform config"
```

## Self-Review

- Spec coverage:
  - Structured config: Tasks 1 and 2.
  - Install flow and secret storage: Tasks 3 and 4.
  - Runtime snapshot and canonical resolver: Tasks 5 and 6.
  - Preflight/runtime alignment and no worker Azure env allowlist: Tasks 5 and 6.
  - Verification and SPA refresh: Task 7.

- Placeholder scan:
  - No `TODO` / `TBD` placeholders remain.
  - Every task names exact files and concrete commands.

- Type consistency:
  - Plan uses `InstallAzureInput` for backend transport, `InstallAzureConfigInput` for generated TS client, `InstallAzurePlatformSelection` for `settings.toml` persistence, and `AzurePlatformConfig` for resolved runtime use.
  - The runtime snapshot file name is consistently `azure-platform.json`.
