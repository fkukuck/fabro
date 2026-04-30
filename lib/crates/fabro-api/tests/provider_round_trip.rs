use std::any::{TypeId, type_name};

use fabro_api::types::Provider as ApiProvider;
use fabro_model::Provider;
use serde_json::json;

#[test]
fn provider_reuses_canonical_type() {
    assert_same_type::<ApiProvider, Provider>();
}

#[test]
fn provider_json_matches_openapi_shape() {
    assert_eq!(
        serde_json::to_value(Provider::Anthropic).unwrap(),
        json!("anthropic")
    );
    assert_eq!(
        serde_json::to_value(Provider::OpenAi).unwrap(),
        json!("openai")
    );
    assert_eq!(
        serde_json::to_value(Provider::OpenAiCompatible).unwrap(),
        json!("openai_compatible")
    );

    assert_eq!(
        serde_json::from_value::<ApiProvider>(json!("inception")).unwrap(),
        Provider::Inception
    );
}

fn assert_same_type<T: 'static, U: 'static>() {
    assert_eq!(
        TypeId::of::<T>(),
        TypeId::of::<U>(),
        "{} should be the same type as {}",
        type_name::<T>(),
        type_name::<U>()
    );
}
