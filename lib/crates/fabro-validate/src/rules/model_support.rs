use std::str::FromStr;

use crate::{Diagnostic, Severity};

pub(super) fn check_model_known(
    rule_name: &str,
    model: &str,
    context: &str,
    node_id: Option<String>,
) -> Option<Diagnostic> {
    if fabro_model::Catalog::builtin().get(model).is_some() {
        return None;
    }
    Some(Diagnostic {
        rule: rule_name.to_string(),
        severity: Severity::Warning,
        message: format!(
            "Unknown model '{model}' {context}. Run `fabro model list` to see available models"
        ),
        node_id,
        edge: None,
        fix: Some("Use a model ID from `fabro model list`".to_string()),
    })
}

pub(super) fn check_provider_known(
    rule_name: &str,
    provider: &str,
    context: &str,
    node_id: Option<String>,
) -> Option<Diagnostic> {
    if fabro_model::Provider::from_str(provider).is_ok() {
        return None;
    }
    let valid: Vec<&str> = fabro_model::Provider::ALL
        .iter()
        .map(|&p| <&'static str>::from(p))
        .collect();
    let valid_str = valid.join(", ");
    Some(Diagnostic {
        rule: rule_name.to_string(),
        severity: Severity::Warning,
        message: format!("Unknown provider '{provider}' {context}. Valid providers: {valid_str}"),
        node_id,
        edge: None,
        fix: Some(format!("Use one of: {valid_str}")),
    })
}
