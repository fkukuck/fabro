use fabro_sandbox::SandboxSpec;
use fabro_sandbox::config::AzureConfig;

#[tokio::test]
#[ignore = "requires Azure credentials and network access"]
async fn azure_exec_command_round_trip() {
    let sandbox = SandboxSpec::Azure {
        config:       AzureConfig {
            image:     Some(std::env::var("FABRO_AZURE_TEST_IMAGE").unwrap()),
            cpu:       Some(2.0),
            memory_gb: Some(4.0),
        },
        github_app:   None,
        run_id:       None,
        clone_branch: None,
    }
    .build(None)
    .await
    .unwrap();

    sandbox.initialize().await.unwrap();
    let result = sandbox
        .exec_command("printf hello", 10_000, None, None, None)
        .await
        .unwrap();
    assert_eq!(result.stdout, "hello");
    sandbox.cleanup().await.unwrap();
}
