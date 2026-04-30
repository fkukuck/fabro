use std::any::{TypeId, type_name};

use fabro_api::types::InternalStageStatus as ApiInternalStageStatus;
use fabro_types::StageStatus;
use serde_json::json;

#[test]
fn internal_stage_status_reuses_canonical_type() {
    assert_same_type::<ApiInternalStageStatus, StageStatus>();
}

#[test]
fn internal_stage_status_serializes_as_snake_case_strings() {
    assert_eq!(
        serde_json::to_value(StageStatus::Success).unwrap(),
        json!("success")
    );
    assert_eq!(
        serde_json::to_value(StageStatus::Fail).unwrap(),
        json!("fail")
    );
    assert_eq!(
        serde_json::to_value(StageStatus::Skipped).unwrap(),
        json!("skipped")
    );
    assert_eq!(
        serde_json::to_value(StageStatus::PartialSuccess).unwrap(),
        json!("partial_success")
    );
    assert_eq!(
        serde_json::to_value(StageStatus::Retry).unwrap(),
        json!("retry")
    );
}

#[test]
fn internal_stage_status_deserializes_each_variant() {
    assert_eq!(
        serde_json::from_value::<StageStatus>(json!("success")).unwrap(),
        StageStatus::Success
    );
    assert_eq!(
        serde_json::from_value::<StageStatus>(json!("fail")).unwrap(),
        StageStatus::Fail
    );
    assert_eq!(
        serde_json::from_value::<StageStatus>(json!("skipped")).unwrap(),
        StageStatus::Skipped
    );
    assert_eq!(
        serde_json::from_value::<StageStatus>(json!("partial_success")).unwrap(),
        StageStatus::PartialSuccess
    );
    assert_eq!(
        serde_json::from_value::<StageStatus>(json!("retry")).unwrap(),
        StageStatus::Retry
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
