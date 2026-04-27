#![cfg(feature = "azure")]

use fabro_config::Storage;
use fabro_sandbox::config::AzureConfig;
use fabro_sandbox::{SandboxSpec, reconnect};
use fabro_static::EnvVars;

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
            sandboxd_port: 7777,
            acr_username: None,
            acr_password: None,
        })
        .unwrap(),
    )
    .unwrap();
}

#[tokio::test]
async fn azure_parallel_worktree_path_uses_workspace_scratch_dir() {
    let dir = tempfile::tempdir().unwrap();
    write_test_azure_snapshot(dir.path());
    temp_env::with_var(EnvVars::FABRO_STORAGE_ROOT, Some(dir.path()), || async {
        let spec = SandboxSpec::Azure {
            config:           AzureConfig {
                image:     Some("fabro.azurecr.io/fabro-sandboxes/base:latest".into()),
                cpu:       Some(2.0),
                memory_gb: Some(4.0),
            },
            github_app:       None,
            run_id:           None,
            clone_origin_url: None,
            clone_branch:     None,
        };
        let sandbox = spec.build(None).await.unwrap();
        let path = sandbox.parallel_worktree_path(
            std::path::Path::new("/tmp/run"),
            "run-1",
            "node-a",
            "left",
        );
        assert_eq!(path, "/workspace/.fabro/scratch/run-1/parallel/node-a/left");
    })
    .await;
}

#[tokio::test]
async fn azure_reconnect_uses_saved_resource_id() {
    let dir = tempfile::tempdir().unwrap();
    write_test_azure_snapshot(dir.path());
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
            "sandboxd_port": 7777,
            "acr_username": null,
            "acr_password": null,
        }))
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

    let sandbox = reconnect::reconnect(&record, None, Some(dir.path()))
        .await
        .unwrap();
    assert_eq!(sandbox.working_directory(), "/workspace");
}
