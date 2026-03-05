use crate::types::ModelInfo;
use std::sync::LazyLock;

/// Built-in model catalog loaded from catalog.json (Section 2.9).
/// The catalog is advisory, not restrictive -- unknown model strings pass through.
static BUILT_IN_MODELS: LazyLock<Vec<ModelInfo>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("catalog.json")).expect("embedded catalog.json must be valid")
});

/// Get model info by model ID (Section 2.9).
#[must_use]
pub fn get_model_info(model_id: &str) -> Option<ModelInfo> {
    BUILT_IN_MODELS
        .iter()
        .find(|m| m.id == model_id || m.aliases.iter().any(|a| a == model_id))
        .cloned()
}

/// Get the default model for a provider, as marked in catalog.json.
///
/// Returns `None` if the provider has no models or none marked as default.
#[must_use]
pub fn default_model_for_provider(provider: &str) -> Option<ModelInfo> {
    BUILT_IN_MODELS
        .iter()
        .find(|m| m.provider == provider && m.default)
        .cloned()
}

/// Get the overall default model (the first model marked `default` in catalog.json).
#[must_use]
pub fn default_model() -> ModelInfo {
    BUILT_IN_MODELS
        .iter()
        .find(|m| m.default)
        .cloned()
        .expect("catalog.json must contain at least one default model")
}

/// List all known models, optionally filtered by provider (Section 2.9).
#[must_use]
pub fn list_models(provider: Option<&str>) -> Vec<ModelInfo> {
    provider.map_or_else(
        || BUILT_IN_MODELS.clone(),
        |p| {
            BUILT_IN_MODELS
                .iter()
                .filter(|m| m.provider == p)
                .cloned()
                .collect()
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::Provider;
    use std::str::FromStr;

    #[test]
    fn every_provider_has_catalog_models() {
        for &provider in Provider::ALL {
            let models = list_models(Some(provider.as_str()));
            assert!(
                !models.is_empty(),
                "Provider {:?} has no models in catalog",
                provider
            );
        }
    }

    #[test]
    fn every_provider_has_exactly_one_default_model() {
        for &provider in Provider::ALL {
            let defaults: Vec<_> = list_models(Some(provider.as_str()))
                .into_iter()
                .filter(|m| m.default)
                .collect();
            assert_eq!(
                defaults.len(),
                1,
                "Provider {:?} should have exactly one default model, found {}: {:?}",
                provider,
                defaults.len(),
                defaults.iter().map(|m| &m.id).collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn default_model_returns_first_catalog_default() {
        let m = default_model();
        assert!(m.default);
    }

    #[test]
    fn default_model_for_provider_returns_correct_model() {
        let m = default_model_for_provider("anthropic").unwrap();
        assert_eq!(m.id, "claude-opus-4-6");
        assert!(m.default);

        let m = default_model_for_provider("openai").unwrap();
        assert_eq!(m.id, "gpt-5.2");

        let m = default_model_for_provider("gemini").unwrap();
        assert_eq!(m.id, "gemini-3.1-pro-preview");

        assert!(default_model_for_provider("nonexistent").is_none());
    }

    #[test]
    fn catalog_provider_strings_roundtrip_through_provider() {
        for model in list_models(None) {
            let parsed = Provider::from_str(&model.provider);
            assert!(
                parsed.is_ok(),
                "catalog model '{}' has provider '{}' which does not parse as Provider",
                model.id,
                model.provider
            );
        }
    }

    #[test]
    fn provider_as_str_roundtrips_through_from_str() {
        for &provider in Provider::ALL {
            let roundtripped = Provider::from_str(provider.as_str());
            assert_eq!(
                roundtripped,
                Ok(provider),
                "Provider::{:?}.as_str() does not round-trip through from_str",
                provider
            );
        }
    }

    #[test]
    fn get_model_info_by_id() {
        let info = get_model_info("claude-opus-4-6").unwrap();
        assert_eq!(info.display_name, "Claude Opus 4.6");
        assert_eq!(info.provider, "anthropic");
        assert!(info.supports_tools);
        assert!(info.supports_vision);
        assert!(info.supports_reasoning);
        assert_eq!(info.context_window, 1_000_000);
        assert_eq!(info.max_output, Some(128_000));
    }

    #[test]
    fn get_model_info_by_alias() {
        let info = get_model_info("opus").unwrap();
        assert_eq!(info.id, "claude-opus-4-6");

        let info = get_model_info("sonnet").unwrap();
        assert_eq!(info.id, "claude-sonnet-4-5");

        let info = get_model_info("codex").unwrap();
        assert_eq!(info.id, "gpt-5.3-codex");
    }

    #[test]
    fn get_model_info_returns_none_for_unknown() {
        assert!(get_model_info("nonexistent-model").is_none());
    }

    #[test]
    fn list_models_by_provider() {
        let anthropic = list_models(Some("anthropic"));
        assert_eq!(anthropic.len(), 3);
        assert!(anthropic.iter().all(|m| m.provider == "anthropic"));

        let openai = list_models(Some("openai"));
        assert_eq!(openai.len(), 4);

        let gemini = list_models(Some("gemini"));
        assert_eq!(gemini.len(), 3);

        let unknown = list_models(Some("unknown"));
        assert!(unknown.is_empty());
    }

    #[test]
    fn gemini_3_1_flash_lite_in_catalog() {
        let m = get_model_info("gemini-3.1-flash-lite-preview").unwrap();
        assert_eq!(m.provider, "gemini");
        assert_eq!(m.display_name, "Gemini 3.1 Flash Lite (Preview)");
        assert_eq!(m.context_window, 1048576);
        assert_eq!(m.max_output, Some(65536));
        assert!(m.supports_tools);
        assert!(m.supports_vision);
        assert!(m.supports_reasoning);
        assert_eq!(m.input_cost_per_million, Some(0.25));
        assert_eq!(m.output_cost_per_million, Some(1.5));
    }

    #[test]
    fn gemini_flash_lite_alias() {
        assert_eq!(
            get_model_info("gemini-flash-lite").unwrap().id,
            "gemini-3.1-flash-lite-preview"
        );
    }

    #[test]
    fn kimi_k2_5_in_catalog() {
        let m = get_model_info("kimi-k2.5").unwrap();
        assert_eq!(m.provider, "kimi");
        assert_eq!(m.max_output, Some(16000));
        assert_eq!(m.context_window, 262144);
    }

    #[test]
    fn kimi_alias() {
        assert_eq!(get_model_info("kimi").unwrap().id, "kimi-k2.5");
    }

    #[test]
    fn glm_4_7_in_catalog() {
        let m = get_model_info("glm-4.7").unwrap();
        assert_eq!(m.provider, "zai");
    }

    #[test]
    fn minimax_m2_5_in_catalog() {
        let m = get_model_info("minimax-m2.5").unwrap();
        assert_eq!(m.provider, "minimax");
    }

    #[test]
    fn mercury_2_in_catalog() {
        let m = get_model_info("mercury-2").unwrap();
        assert_eq!(m.provider, "inception");
        assert_eq!(m.context_window, 131072);
        assert_eq!(m.max_output, Some(50000));
        assert!(m.supports_tools);
        assert!(!m.supports_vision);
        assert!(m.supports_reasoning);
    }

    #[test]
    fn mercury_alias_resolves_to_mercury_2() {
        assert_eq!(get_model_info("mercury").unwrap().id, "mercury-2");
    }

    #[test]
    fn model_info_costs() {
        let claude = get_model_info("claude-opus-4-6").unwrap();
        assert_eq!(claude.input_cost_per_million, Some(15.0));
        assert_eq!(claude.output_cost_per_million, Some(75.0));

        let sonnet = get_model_info("claude-sonnet-4-5").unwrap();
        assert_eq!(sonnet.input_cost_per_million, Some(3.0));
    }
}
