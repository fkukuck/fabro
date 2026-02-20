pub mod anthropic;
pub mod gemini;
pub mod openai;

pub use anthropic::AnthropicProfile;
pub use gemini::GeminiProfile;
pub use openai::OpenAiProfile;

use crate::execution_env::ExecutionEnvironment;

/// Additional context for building environment blocks
#[derive(Default)]
pub struct EnvContext {
    pub git_branch: Option<String>,
    pub is_git_repo: bool,
    pub date: String,
    pub model_name: String,
}

#[must_use]
pub fn build_env_context_block(env: &dyn ExecutionEnvironment) -> String {
    build_env_context_block_with(env, &EnvContext::default())
}

#[must_use]
pub fn build_env_context_block_with(env: &dyn ExecutionEnvironment, ctx: &EnvContext) -> String {
    let mut lines = vec![
        "# Environment".to_string(),
        format!("- Working directory: {}", env.working_directory()),
        format!("- Platform: {}", env.platform()),
        format!("- OS: {}", env.os_version()),
    ];

    if ctx.is_git_repo {
        lines.push(format!("- Is a git repository: {}", ctx.is_git_repo));
    }
    if let Some(ref branch) = ctx.git_branch {
        lines.push(format!("- Git branch: {branch}"));
    }
    if !ctx.date.is_empty() {
        lines.push(format!("- Date: {}", ctx.date));
    }
    if !ctx.model_name.is_empty() {
        lines.push(format!("- Model: {}", ctx.model_name));
    }

    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution_env::*;
    use async_trait::async_trait;

    struct TestEnv;

    #[async_trait]
    impl ExecutionEnvironment for TestEnv {
        async fn read_file(&self, _: &str) -> Result<String, String> {
            Ok(String::new())
        }
        async fn write_file(&self, _: &str, _: &str) -> Result<(), String> {
            Ok(())
        }
        async fn file_exists(&self, _: &str) -> Result<bool, String> {
            Ok(false)
        }
        async fn list_directory(&self, _: &str) -> Result<Vec<DirEntry>, String> {
            Ok(vec![])
        }
        async fn exec_command(
            &self,
            _: &str,
            _: &[String],
            _: u64,
            _: Option<&str>,
            _: Option<&std::collections::HashMap<String, String>>,
        ) -> Result<ExecResult, String> {
            Ok(ExecResult {
                stdout: String::new(),
                stderr: String::new(),
                exit_code: 0,
                timed_out: false,
                duration_ms: 0,
            })
        }
        async fn grep(
            &self,
            _: &str,
            _: &str,
            _: &GrepOptions,
        ) -> Result<Vec<String>, String> {
            Ok(vec![])
        }
        async fn glob(&self, _: &str) -> Result<Vec<String>, String> {
            Ok(vec![])
        }
        async fn initialize(&self) -> Result<(), String> {
            Ok(())
        }
        async fn cleanup(&self) -> Result<(), String> {
            Ok(())
        }
        fn working_directory(&self) -> &str {
            "/home/test"
        }
        fn platform(&self) -> &str {
            "linux"
        }
        fn os_version(&self) -> String {
            "Linux 6.1.0".into()
        }
    }

    #[test]
    fn env_context_block_contains_platform() {
        let env = TestEnv;
        let block = build_env_context_block(&env);
        assert!(block.contains("# Environment"));
        assert!(block.contains("linux"));
        assert!(block.contains("/home/test"));
        assert!(block.contains("Linux 6.1.0"));
    }

    #[test]
    fn env_context_block_with_extra_context() {
        let env = TestEnv;
        let ctx = EnvContext {
            git_branch: Some("main".into()),
            is_git_repo: true,
            date: "2026-02-20".into(),
            model_name: "claude-opus-4-6".into(),
        };
        let block = build_env_context_block_with(&env, &ctx);
        assert!(block.contains("Git branch: main"));
        assert!(block.contains("Is a git repository: true"));
        assert!(block.contains("Date: 2026-02-20"));
        assert!(block.contains("Model: claude-opus-4-6"));
    }
}
