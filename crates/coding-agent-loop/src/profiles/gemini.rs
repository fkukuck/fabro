use crate::execution_env::ExecutionEnvironment;
use crate::provider_profile::ProviderProfile;
use crate::tool_registry::ToolRegistry;
use crate::tools::{
    make_edit_file_tool, make_glob_tool, make_grep_tool, make_read_file_tool, make_shell_tool,
    make_write_file_tool,
};
use unified_llm::types::ToolDefinition;

use super::build_env_context_block;

pub struct GeminiProfile {
    model: String,
    registry: ToolRegistry,
}

impl GeminiProfile {
    #[must_use]
    pub fn new(model: impl Into<String>) -> Self {
        let mut registry = ToolRegistry::new();

        registry.register(make_read_file_tool());
        registry.register(make_write_file_tool());
        registry.register(make_edit_file_tool());
        registry.register(make_shell_tool());
        registry.register(make_grep_tool());
        registry.register(make_glob_tool());

        Self {
            model: model.into(),
            registry,
        }
    }
}

impl ProviderProfile for GeminiProfile {
    fn id(&self) -> String {
        "gemini".into()
    }

    fn model(&self) -> String {
        self.model.clone()
    }

    fn tool_registry(&self) -> &ToolRegistry {
        &self.registry
    }

    fn tool_registry_mut(&mut self) -> &mut ToolRegistry {
        &mut self.registry
    }

    fn build_system_prompt(
        &self,
        env: &dyn ExecutionEnvironment,
        project_docs: &[String],
    ) -> String {
        let env_block = build_env_context_block(env);
        let docs_section = if project_docs.is_empty() {
            String::new()
        } else {
            format!("\n\n{}", project_docs.join("\n\n"))
        };

        format!(
            "You are a coding assistant powered by Gemini. You help users with software engineering tasks.\n\n\
             {env_block}\n\n\
             # Tools\n\
             Use the provided tools to interact with the codebase and environment.\
             {docs_section}"
        )
    }

    fn tools(&self) -> Vec<ToolDefinition> {
        self.registry.definitions()
    }

    fn provider_options(&self) -> Option<serde_json::Value> {
        None
    }

    fn supports_reasoning(&self) -> bool {
        true
    }

    fn supports_streaming(&self) -> bool {
        true
    }

    fn supports_parallel_tool_calls(&self) -> bool {
        true
    }

    fn context_window_size(&self) -> usize {
        1_000_000
    }
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
    fn gemini_profile_identity() {
        let profile = GeminiProfile::new("gemini-2.0-flash");
        assert_eq!(profile.id(), "gemini");
        assert_eq!(profile.model(), "gemini-2.0-flash");
    }

    #[test]
    fn gemini_profile_capabilities() {
        let profile = GeminiProfile::new("gemini-2.0-flash");
        assert!(profile.supports_reasoning());
        assert!(profile.supports_streaming());
        assert!(profile.supports_parallel_tool_calls());
        assert_eq!(profile.context_window_size(), 1_000_000);
    }

    #[test]
    fn gemini_system_prompt_contains_env_context() {
        let profile = GeminiProfile::new("gemini-2.0-flash");
        let env = TestEnv;
        let prompt = profile.build_system_prompt(&env, &[]);
        assert!(prompt.contains("powered by Gemini"));
        assert!(prompt.contains("# Environment"));
        assert!(prompt.contains("linux"));
    }

    #[test]
    fn gemini_tools_registered() {
        let profile = GeminiProfile::new("gemini-2.0-flash");
        let names = profile.tool_registry().names();
        assert_eq!(names.len(), 6);
        assert!(names.contains(&"read_file".to_string()));
        assert!(names.contains(&"write_file".to_string()));
        assert!(names.contains(&"edit_file".to_string()));
        assert!(names.contains(&"shell".to_string()));
        assert!(names.contains(&"grep".to_string()));
        assert!(names.contains(&"glob".to_string()));
    }
}
