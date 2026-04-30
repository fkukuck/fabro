use std::any::{TypeId, type_name};

use fabro_api::types::ModelFeatures as ApiModelFeatures;
use fabro_model::ModelFeatures;

#[test]
fn model_features_reuses_canonical_type() {
    assert_same_type::<ApiModelFeatures, ModelFeatures>();
}

#[test]
fn model_features_json_matches_openapi_shape() {
    let features = ModelFeatures {
        tools:     true,
        vision:    true,
        reasoning: true,
        effort:    false,
    };

    let json = serde_json::to_value(&features).unwrap();
    assert_eq!(json["tools"], true);
    assert_eq!(json["vision"], true);
    assert_eq!(json["reasoning"], true);
    assert_eq!(json["effort"], false);

    let round_trip: ApiModelFeatures = serde_json::from_value(json).unwrap();
    assert_eq!(round_trip, features);
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
