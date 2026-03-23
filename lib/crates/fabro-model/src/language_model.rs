use crate::provider::Provider;
use crate::types::ModelInfo;

/// Trait abstracting over model metadata. Implemented by `ModelInfo` via blanket impl,
/// and intended as the primary interface for querying model capabilities.
pub trait LanguageModel: Send + Sync + std::fmt::Debug {
    fn id(&self) -> &str;
    fn provider(&self) -> Provider;
    fn family(&self) -> &str;
    fn display_name(&self) -> &str;
    fn context_window(&self) -> i64;
    fn max_output(&self) -> Option<i64>;
    fn supports_tools(&self) -> bool;
    fn supports_vision(&self) -> bool;
    fn supports_reasoning(&self) -> bool;
    fn supports_effort(&self) -> bool;
    fn training(&self) -> Option<&str>;
    fn input_cost_per_mtok(&self) -> Option<f64>;
    fn output_cost_per_mtok(&self) -> Option<f64>;
    fn cache_input_cost_per_mtok(&self) -> Option<f64>;
    fn estimated_output_tps(&self) -> Option<f64>;
    fn aliases(&self) -> &[String];
    fn is_default(&self) -> bool;
    fn to_model_info(&self) -> ModelInfo;
}

impl LanguageModel for ModelInfo {
    fn id(&self) -> &str {
        &self.id
    }

    fn provider(&self) -> Provider {
        self.provider
            .parse::<Provider>()
            .unwrap_or(Provider::Anthropic)
    }

    fn family(&self) -> &str {
        &self.family
    }

    fn display_name(&self) -> &str {
        &self.display_name
    }

    fn context_window(&self) -> i64 {
        self.limits.context_window
    }

    fn max_output(&self) -> Option<i64> {
        self.limits.max_output
    }

    fn supports_tools(&self) -> bool {
        self.features.tools
    }

    fn supports_vision(&self) -> bool {
        self.features.vision
    }

    fn supports_reasoning(&self) -> bool {
        self.features.reasoning
    }

    fn supports_effort(&self) -> bool {
        self.features.effort
    }

    fn training(&self) -> Option<&str> {
        self.training.as_deref()
    }

    fn input_cost_per_mtok(&self) -> Option<f64> {
        self.costs.input_cost_per_mtok
    }

    fn output_cost_per_mtok(&self) -> Option<f64> {
        self.costs.output_cost_per_mtok
    }

    fn cache_input_cost_per_mtok(&self) -> Option<f64> {
        self.costs.cache_input_cost_per_mtok
    }

    fn estimated_output_tps(&self) -> Option<f64> {
        self.estimated_output_tps
    }

    fn aliases(&self) -> &[String] {
        &self.aliases
    }

    fn is_default(&self) -> bool {
        self.default
    }

    fn to_model_info(&self) -> ModelInfo {
        self.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::Catalog;

    #[test]
    fn trait_is_object_safe() {
        let info = Catalog::builtin().get("claude-opus-4-6").unwrap().clone();
        let boxed: Box<dyn LanguageModel> = Box::new(info);
        assert_eq!(boxed.id(), "claude-opus-4-6");
    }

    #[test]
    fn blanket_impl_returns_correct_values() {
        let info = Catalog::builtin().get("claude-opus-4-6").unwrap();
        assert_eq!(info.id(), "claude-opus-4-6");
        assert_eq!(info.provider(), Provider::Anthropic);
        assert_eq!(info.family(), "claude-4");
        assert_eq!(info.display_name(), "Claude Opus 4.6");
        assert_eq!(info.context_window(), 1_000_000);
        assert_eq!(info.max_output(), Some(128_000));
        assert!(info.supports_tools());
        assert!(info.supports_vision());
        assert!(info.supports_reasoning());
        assert!(info.supports_effort());
        assert_eq!(info.training(), Some("2025-08-01"));
        assert_eq!(info.input_cost_per_mtok(), Some(15.0));
        assert_eq!(info.output_cost_per_mtok(), Some(75.0));
        assert_eq!(info.cache_input_cost_per_mtok(), Some(1.5));
        assert_eq!(info.estimated_output_tps(), Some(25.0));
        assert!(!info.aliases().is_empty());
        assert!(!info.is_default());
    }

    #[test]
    fn all_catalog_providers_roundtrip() {
        for model in Catalog::builtin().list(None) {
            // Should not panic — every catalog model's provider string must parse
            let _ = model.provider();
        }
    }

    #[test]
    fn to_model_info_roundtrips() {
        let info = Catalog::builtin().get("claude-opus-4-6").unwrap().clone();
        let roundtripped = info.to_model_info();
        assert_eq!(info, roundtripped);
    }
}
