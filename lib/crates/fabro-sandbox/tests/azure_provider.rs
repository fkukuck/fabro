#![cfg(feature = "azure")]

use fabro_config::Storage;
use fabro_http::test_http_client;
use fabro_sandbox::azure::AzureSandbox;
use fabro_sandbox::azure::arm::AzureArmClient;
use fabro_sandbox::config::AzureConfig;
use fabro_sandbox::{Sandbox, reconnect};
use fabro_static::EnvVars;
use httpmock::prelude::{GET, MockServer, PUT};

fn write_test_azure_snapshot(root: &std::path::Path) {
    let runtime = Storage::new(root).runtime_directory();
    std::fs::write(
        runtime.azure_platform_config_path(),
        serde_json::to_vec(&fabro_sandbox::azure::config::AzurePlatformConfig {
            subscription_id: "sub-1".into(),
            resource_group: "rg-1".into(),
            location: "eastus".into(),
            subnet_id:
                "/subscriptions/sub-1/resourceGroups/rg-1/providers/Microsoft.Network/virtualNetworks/vnet-1/subnets/aci"
                    .into(),
            acr_server: "fabro.azurecr.io".into(),
            acr_identity_resource_id: "/subscriptions/sub-1/resourceGroups/rg-1/providers/Microsoft.ManagedIdentity/userAssignedIdentities/acr-pull".into(),
            sandboxd_port: 7777,
        })
        .unwrap(),
    )
    .unwrap();
}

#[test]
fn azure_arm_create_container_group_uses_managed_identity_for_acr_pulls() {
    let server = MockServer::start();
    let token_mock = server.mock(|when, then| {
        when.method(GET)
            .path("/metadata/identity/oauth2/token")
            .header("x-identity-header", "test-header")
            .query_param("api-version", "2019-08-01")
            .query_param("resource", "https://management.azure.com/");
        then.status(200).json_body(serde_json::json!({
            "access_token": "test-token"
        }));
    });
    let create_mock = server.mock(|when, then| {
        when.method(PUT)
            .path("/subscriptions/sub-1/resourceGroups/rg-1/providers/Microsoft.ContainerInstance/containerGroups/fabro-run-1")
            .query_param("api-version", "2023-05-01")
            .header("authorization", "Bearer test-token")
            .body_includes(r#""identity":{"type":"UserAssigned""#)
            .body_includes(r#""/subscriptions/sub-1/resourceGroups/rg-1/providers/Microsoft.ManagedIdentity/userAssignedIdentities/acr-pull":{}"#)
            .body_includes(r#""imageRegistryCredentials":[{"server":"fabro.azurecr.io","identity":"/subscriptions/sub-1/resourceGroups/rg-1/providers/Microsoft.ManagedIdentity/userAssignedIdentities/acr-pull"}]"#)
            .body_includes(r#""subnetIds":[{"id":"/subscriptions/sub-1/resourceGroups/rg-1/providers/Microsoft.Network/virtualNetworks/vnet-1/subnets/aci"}]"#)
            .body_includes(r#""mountPath":"/workspace""#)
            .body_includes(r#""emptyDir":{}"#)
            .body_includes(r#""port":7777"#);
        then.status(200);
    });
    let config = fabro_sandbox::azure::config::AzurePlatformConfig {
        subscription_id: "sub-1".into(),
        resource_group: "rg-1".into(),
        location: "eastus".into(),
        subnet_id:
            "/subscriptions/sub-1/resourceGroups/rg-1/providers/Microsoft.Network/virtualNetworks/vnet-1/subnets/aci"
                .into(),
        acr_server: "fabro.azurecr.io".into(),
        acr_identity_resource_id: "/subscriptions/sub-1/resourceGroups/rg-1/providers/Microsoft.ManagedIdentity/userAssignedIdentities/acr-pull".into(),
        sandboxd_port: 7777,
    };
    let arm =
        AzureArmClient::new_with_base_url(test_http_client().unwrap(), config, server.base_url());

    temp_env::with_vars(
        vec![
            (
                "IDENTITY_ENDPOINT",
                Some(format!(
                    "{}/metadata/identity/oauth2/token",
                    server.base_url()
                )),
            ),
            ("IDENTITY_HEADER", Some("test-header".to_string())),
            ("AZURE_CLIENT_ID", None::<String>),
        ],
        || {
            tokio::runtime::Runtime::new().unwrap().block_on(async {
                arm.create_container_group(
                    "fabro-run-1",
                    "fabro.azurecr.io/fabro-sandboxes/base:latest",
                    2.0,
                    4.0,
                )
                .await
                .unwrap();
            });
        },
    );

    tokio::runtime::Runtime::new().unwrap().block_on(async {
        token_mock.assert_async().await;
        create_mock.assert_async().await;
    });
}

#[test]
fn azure_parallel_worktree_path_uses_workspace_scratch_dir() {
    let dir = tempfile::tempdir().unwrap();
    write_test_azure_snapshot(dir.path());
    temp_env::with_var(EnvVars::FABRO_STORAGE_ROOT, Some(dir.path()), || {
        let sandbox = AzureSandbox::new(
            AzureConfig {
                image:     Some("fabro.azurecr.io/fabro-sandboxes/base:latest".into()),
                cpu:       Some(2.0),
                memory_gb: Some(4.0),
                platform:  None,
            },
            None,
            None,
            None,
            None,
        )
        .unwrap();
        let path = sandbox.parallel_worktree_path(
            std::path::Path::new("/tmp/run"),
            "run-1",
            "node-a",
            "left",
        );
        assert_eq!(path, "/workspace/.fabro/scratch/run-1/parallel/node-a/left");
    });
}

#[test]
fn azure_sandbox_new_uses_settings_platform_without_snapshot_file() {
    let platform = fabro_sandbox::azure::config::AzurePlatformConfig {
        subscription_id:          "sub-1".into(),
        resource_group:           "rg-1".into(),
        location:                 "eastus".into(),
        subnet_id:                "/subscriptions/sub-1/.../aci".into(),
        acr_server:               "fabro.azurecr.io".into(),
        acr_identity_resource_id: "/subscriptions/sub-1/resourceGroups/rg-1/providers/Microsoft.ManagedIdentity/userAssignedIdentities/acr-pull".into(),
        sandboxd_port:            7777,
    };

    temp_env::with_var(EnvVars::FABRO_STORAGE_ROOT, None::<&str>, || {
        AzureSandbox::new(
            AzureConfig {
                image: Some("fabro.azurecr.io/fabro-sandboxes/base:latest".into()),
                platform: Some(platform),
                ..AzureConfig::default()
            },
            None,
            None,
            None,
            None,
        )
        .unwrap();
    });
}

#[tokio::test]
async fn azure_reconnect_uses_saved_resource_id() {
    let dir = tempfile::tempdir().unwrap();
    write_test_azure_snapshot(dir.path());
    let record = fabro_sandbox::SandboxRecord {
        provider: "azure".to_string(),
        working_directory: "/workspace".to_string(),
        identifier: Some("/subscriptions/sub-1/resourceGroups/rg-1/providers/Microsoft.ContainerInstance/containerGroups/fabro-run-1".to_string()),
        repo_cloned: None,
        clone_origin_url: None,
        clone_branch: None,
    };
    let sandbox = reconnect::reconnect(&record, None, Some(dir.path()))
        .await
        .unwrap();
    assert_eq!(sandbox.working_directory(), "/workspace");
}

#[tokio::test]
async fn azure_reconnect_loads_platform_snapshot_from_storage_root() {
    let dir = tempfile::tempdir().unwrap();
    let runtime = Storage::new(dir.path()).runtime_directory();
    std::fs::write(
        runtime.azure_platform_config_path(),
        serde_json::to_vec(&serde_json::json!({
            "subscription_id": "sub-1",
            "resource_group": "rg-1",
            "location": "eastus",
            "subnet_id": "/subscriptions/sub-1/.../aci",
            "acr_server": "fabro.azurecr.io",
            "acr_identity_resource_id": "/subscriptions/sub-1/resourceGroups/rg-1/providers/Microsoft.ManagedIdentity/userAssignedIdentities/acr-pull",
            "sandboxd_port": 7777,
        }))
        .unwrap(),
    )
    .unwrap();
    let record = fabro_sandbox::SandboxRecord {
        provider: "azure".to_string(),
        working_directory: "/workspace".to_string(),
        identifier: Some("/subscriptions/sub-1/resourceGroups/rg-1/providers/Microsoft.ContainerInstance/containerGroups/fabro-run-1".to_string()),
        repo_cloned: None,
        clone_origin_url: None,
        clone_branch: None,
    };

    let sandbox = reconnect::reconnect(&record, None, Some(dir.path()))
        .await
        .unwrap();
    assert_eq!(sandbox.working_directory(), "/workspace");
}

#[tokio::test]
async fn azure_reconnect_loads_legacy_platform_snapshot_from_storage_root() {
    let dir = tempfile::tempdir().unwrap();
    let runtime = Storage::new(dir.path()).runtime_directory();
    std::fs::write(
        runtime.azure_platform_config_path(),
        serde_json::to_vec(&serde_json::json!({
            "subscription_id": "sub-1",
            "resource_group": "rg-1",
            "location": "eastus",
            "subnet_id": "/subscriptions/sub-1/.../aci",
            "acr_server": "fabro.azurecr.io",
            "sandboxd_port": 7777,
            "acr_username": "legacy-user",
            "acr_password": "legacy-pass"
        }))
        .unwrap(),
    )
    .unwrap();
    let record = fabro_sandbox::SandboxRecord {
        provider: "azure".to_string(),
        working_directory: "/workspace".to_string(),
        identifier: Some("/subscriptions/sub-1/resourceGroups/rg-1/providers/Microsoft.ContainerInstance/containerGroups/fabro-run-1".to_string()),
        repo_cloned: None,
        clone_origin_url: None,
        clone_branch: None,
    };

    let sandbox = reconnect::reconnect(&record, None, Some(dir.path()))
        .await
        .unwrap();
    assert_eq!(sandbox.working_directory(), "/workspace");
}
