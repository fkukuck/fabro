use std::any::{TypeId, type_name};

use chrono::{TimeZone, Utc};
use fabro_api::types::{
    BlockedReason as ApiBlockedReason, RunControlAction as ApiRunControlAction,
    RunStatus as ApiRunStatus, RunStatusRecord as ApiRunStatusRecord,
    StatusReason as ApiStatusReason,
};
use fabro_types::status::{
    BlockedReason, RunControlAction, RunStatus, RunStatusRecord, StatusReason,
};
use serde::Serialize;
use serde_json::{Value, json};

#[test]
fn status_family_reuses_domain_types() {
    assert_same_type::<ApiRunStatus, RunStatus>();
    assert_same_type::<ApiStatusReason, StatusReason>();
    assert_same_type::<ApiBlockedReason, BlockedReason>();
    assert_same_type::<ApiRunControlAction, RunControlAction>();
    assert_same_type::<ApiRunStatusRecord, RunStatusRecord>();
}

// The `status_family_reuses_domain_types` assertions above prove each API type
// is the same type as its domain counterpart, so each variant below only needs
// to be asserted once to lock in the OpenAPI string token.

#[test]
fn run_status_json_tokens_match_openapi() {
    assert_string_json(RunStatus::Submitted, "submitted");
    assert_string_json(RunStatus::Queued, "queued");
    assert_string_json(RunStatus::Starting, "starting");
    assert_string_json(RunStatus::Running, "running");
    assert_string_json(RunStatus::Blocked, "blocked");
    assert_string_json(RunStatus::Paused, "paused");
    assert_string_json(RunStatus::Removing, "removing");
    assert_string_json(RunStatus::Succeeded, "succeeded");
    assert_string_json(RunStatus::Failed, "failed");
    assert_string_json(RunStatus::Dead, "dead");
    assert_string_json(RunStatus::Archived, "archived");
}

#[test]
fn status_reason_json_tokens_match_openapi() {
    assert_string_json(StatusReason::Completed, "completed");
    assert_string_json(StatusReason::PartialSuccess, "partial_success");
    assert_string_json(StatusReason::WorkflowError, "workflow_error");
    assert_string_json(StatusReason::Cancelled, "cancelled");
    assert_string_json(StatusReason::Terminated, "terminated");
    assert_string_json(StatusReason::TransientInfra, "transient_infra");
    assert_string_json(StatusReason::BudgetExhausted, "budget_exhausted");
    assert_string_json(StatusReason::LaunchFailed, "launch_failed");
    assert_string_json(StatusReason::BootstrapFailed, "bootstrap_failed");
    assert_string_json(StatusReason::SandboxInitFailed, "sandbox_init_failed");
    assert_string_json(StatusReason::SandboxInitializing, "sandbox_initializing");
}

#[test]
fn blocked_reason_json_tokens_match_openapi() {
    assert_string_json(BlockedReason::HumanInputRequired, "human_input_required");
}

#[test]
fn run_control_action_json_tokens_match_openapi() {
    assert_string_json(RunControlAction::Cancel, "cancel");
    assert_string_json(RunControlAction::Pause, "pause");
    assert_string_json(RunControlAction::Unpause, "unpause");
}

#[test]
fn run_status_record_json_matches_openapi_shape() {
    let updated_at = Utc
        .with_ymd_and_hms(2026, 1, 2, 3, 4, 5)
        .single()
        .expect("fixed timestamp should be valid");
    let expected = json!({
        "status": "failed",
        "status_reason": "cancelled",
        "blocked_reason": "human_input_required",
        "updated_at": "2026-01-02T03:04:05Z"
    });

    let record = RunStatusRecord {
        status: RunStatus::Failed,
        status_reason: Some(StatusReason::Cancelled),
        blocked_reason: Some(BlockedReason::HumanInputRequired),
        updated_at,
    };
    assert_eq!(serde_json::to_value(&record).unwrap(), expected);

    let round_trip: RunStatusRecord = serde_json::from_value(expected).unwrap();
    assert_eq!(round_trip.status, RunStatus::Failed);
    assert_eq!(round_trip.status_reason, Some(StatusReason::Cancelled));
    assert_eq!(
        round_trip.blocked_reason,
        Some(BlockedReason::HumanInputRequired)
    );
    assert_eq!(round_trip.updated_at, updated_at);
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

fn assert_string_json<T: Serialize>(value: T, expected: &str) {
    assert_eq!(
        serde_json::to_value(value).unwrap(),
        Value::String(expected.into())
    );
}
