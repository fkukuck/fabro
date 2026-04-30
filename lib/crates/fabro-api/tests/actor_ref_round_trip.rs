use std::any::{TypeId, type_name};

use fabro_api::types::ActorRef as ApiActorRef;
use fabro_types::{ActorKind, ActorRef};
use serde_json::json;

#[test]
fn actor_ref_reuses_canonical_type() {
    assert_same_type::<ApiActorRef, ActorRef>();
}

#[test]
fn actor_ref_round_trips_representative_json() {
    let value = json!({
        "kind": "agent",
        "id": "agent-1",
        "display": "Agent 1"
    });

    let actor: ActorRef = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(actor, ActorRef {
        kind:    ActorKind::Agent,
        id:      Some("agent-1".to_string()),
        display: Some("Agent 1".to_string()),
    });
    assert_eq!(serde_json::to_value(actor).unwrap(), value);
}

#[test]
fn actor_ref_omits_absent_optional_fields() {
    let actor = ActorRef {
        kind:    ActorKind::System,
        id:      None,
        display: None,
    };

    assert_eq!(
        serde_json::to_value(actor).unwrap(),
        json!({"kind": "system"})
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
