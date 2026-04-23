use fabro_sandbox::config::AzureConfig;
use fabro_sandbox::{SandboxSpec, reconnect};

fn configure_test_azure_env() {
    std::env::set_var("FABRO_AZURE_SUBSCRIPTION_ID", "sub-1");
    std::env::set_var("FABRO_AZURE_RESOURCE_GROUP", "rg-1");
    std::env::set_var("FABRO_AZURE_LOCATION", "eastus");
    std::env::set_var(
        "FABRO_AZURE_SANDBOX_SUBNET_ID",
        "/subscriptions/sub-1/resourceGroups/rg-1/providers/Microsoft.Network/virtualNetworks/vnet-1/subnets/aci",
    );
    std::env::set_var("FABRO_AZURE_ACR_SERVER", "fabro.azurecr.io");
}

#[tokio::test]
async fn azure_parallel_worktree_path_uses_workspace_scratch_dir() {
    configure_test_azure_env();
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
    let path =
        sandbox.parallel_worktree_path(std::path::Path::new("/tmp/run"), "run-1", "node-a", "left");
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
    let sandbox = reconnect::reconnect(&record, None).await.unwrap();
    assert_eq!(sandbox.working_directory(), "/workspace");
}
