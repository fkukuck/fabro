use std::any::{TypeId, type_name};

use fabro_api::types::ActorKind as ApiActorKind;
use fabro_types::ActorKind;
use serde_json::json;

#[test]
fn actor_kind_reuses_canonical_type() {
    assert_same_type::<ApiActorKind, ActorKind>();
}

#[test]
fn actor_kind_serializes_as_snake_case_strings() {
    assert_eq!(
        serde_json::to_value(ActorKind::User).unwrap(),
        json!("user")
    );
    assert_eq!(
        serde_json::to_value(ActorKind::Agent).unwrap(),
        json!("agent")
    );
    assert_eq!(
        serde_json::to_value(ActorKind::System).unwrap(),
        json!("system")
    );
}

#[test]
fn actor_kind_deserializes_each_variant() {
    assert_eq!(
        serde_json::from_value::<ActorKind>(json!("user")).unwrap(),
        ActorKind::User
    );
    assert_eq!(
        serde_json::from_value::<ActorKind>(json!("agent")).unwrap(),
        ActorKind::Agent
    );
    assert_eq!(
        serde_json::from_value::<ActorKind>(json!("system")).unwrap(),
        ActorKind::System
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
