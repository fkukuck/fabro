use std::collections::HashMap;
use std::sync::Arc;

use axum::http::HeaderMap;
use chrono::Utc;
use fabro_automation::{Automation, AutomationTrigger, GithubIssueTrigger};
use fabro_store::{
    AutomationEventSource, AutomationTriggerKind, AutomationTriggerRunContext,
    AutomationTriggerRunRecord, AutomationTriggerRunStatus, GithubIssueTriggerCycleKey,
    TriggerStartDecision,
};
use fabro_types::{GithubIssueRunSource, Principal, RunId, RunSourceContext};
use tracing::warn;

use crate::automation_materializer::AutomationRunMaterializeError;
use crate::automation_runner::{
    FireAutomationRunError, FireAutomationRunInput, fire_automation_run,
};
use crate::github_issue_events::{
    GithubIssueRunInputs, GithubIssueWebhookEvent, parse_issue_event,
};
use crate::server::AppState;

pub(crate) async fn handle_github_issue_webhook(
    state: Arc<AppState>,
    headers: HeaderMap,
    delivery_id: String,
    body: &[u8],
) {
    let Some(event_type) = headers
        .get("x-github-event")
        .and_then(|value| value.to_str().ok())
    else {
        return;
    };
    if event_type != "issues" {
        return;
    }

    let Ok(event) = parse_issue_event(body) else {
        warn!(delivery_id, "Failed to parse GitHub issue webhook payload");
        return;
    };
    if event.is_pull_request() {
        return;
    }

    match event.action.as_str() {
        "labeled" => handle_labeled(state, headers, delivery_id, event).await,
        "unlabeled" => handle_unlabeled(state, delivery_id, event).await,
        _ => {}
    }
}

async fn handle_labeled(
    state: Arc<AppState>,
    headers: HeaderMap,
    delivery_id: String,
    event: GithubIssueWebhookEvent,
) {
    let matches = matching_github_issue_triggers(&state.automation_store().list().await, &event);
    let needs_comment = matches.iter().any(|(_, trigger)| trigger.comment);
    let comment_token = if needs_comment {
        match github_token(&state, &event).await {
            Ok(token) => Some(token),
            Err(err) => {
                warn!(error = %err, delivery_id, "Failed to resolve GitHub issue automation comment token");
                None
            }
        }
    } else {
        None
    };

    for (automation, trigger) in matches {
        let key = cycle_key(&automation, &trigger, &event);
        let trigger_event_id = trigger_event_id(&delivery_id, &automation, &trigger);
        let Ok(trigger_runs) = state.store_ref().automation_trigger_runs().await else {
            warn!(delivery_id, "Failed to open automation trigger run store");
            continue;
        };
        let decision = trigger_runs
            .decide_github_issue_start(&key, &trigger_event_id)
            .await;
        let Ok(TriggerStartDecision::Start { .. }) = decision else {
            if let Err(err) = decision {
                warn!(error = %err, delivery_id, "Failed to decide GitHub issue automation trigger start");
            }
            continue;
        };

        let inputs = event.run_inputs(&trigger.trigger_label, &delivery_id);
        let fired = fire_automation_run(Arc::clone(&state), FireAutomationRunInput {
            automation:      automation.clone(),
            trigger_id:      trigger.id.clone(),
            actor:           Principal::Webhook {
                delivery_id: delivery_id.clone(),
            },
            headers:         headers.clone(),
            input_overrides: issue_inputs_to_toml(&inputs),
            title_override:  Some(event.issue.title.clone()),
            source_context:  Some(RunSourceContext::GithubIssue(GithubIssueRunSource {
                repository:   event.repository.full_name.clone(),
                issue_number: event.issue.number,
                issue_title:  event.issue.title.clone(),
                issue_url:    event.issue.html_url.clone(),
            })),
        })
        .await;

        match fired {
            Ok(fired) => {
                let run_id = Some(fired.created.run_id);
                if fired.start_result.is_ok() {
                    record_trigger_run(
                        &state,
                        &automation,
                        &trigger,
                        &event,
                        &trigger_event_id,
                        run_id,
                        AutomationTriggerRunStatus::Started,
                    )
                    .await;
                    if trigger.comment {
                        post_success_comment(
                            &state,
                            comment_token.as_deref(),
                            &event,
                            &automation.name,
                            run_id.unwrap(),
                            fired.created.summary.links.web.as_deref(),
                        )
                        .await;
                    }
                } else {
                    record_trigger_run(
                        &state,
                        &automation,
                        &trigger,
                        &event,
                        &trigger_event_id,
                        run_id,
                        AutomationTriggerRunStatus::FailedToStart,
                    )
                    .await;
                    if trigger.comment {
                        post_failure_comment(
                            &state,
                            comment_token.as_deref(),
                            &event,
                            failure_comment(&automation.name, "run start failed"),
                        )
                        .await;
                    }
                }
            }
            Err(err) => {
                warn!(error = ?err, delivery_id, "Failed to create GitHub issue automation run");
                record_trigger_run(
                    &state,
                    &automation,
                    &trigger,
                    &event,
                    &trigger_event_id,
                    None,
                    AutomationTriggerRunStatus::FailedToStart,
                )
                .await;
                if trigger.comment {
                    let reason = fire_automation_run_failure_reason(&err);
                    post_failure_comment(
                        &state,
                        comment_token.as_deref(),
                        &event,
                        failure_comment(&automation.name, reason),
                    )
                    .await;
                }
            }
        }
    }
}

async fn handle_unlabeled(
    state: Arc<AppState>,
    delivery_id: String,
    event: GithubIssueWebhookEvent,
) {
    let matches = matching_github_issue_triggers(&state.automation_store().list().await, &event);
    for (automation, trigger) in matches {
        let trigger_runs = match state.store_ref().automation_trigger_runs().await {
            Ok(trigger_runs) => trigger_runs,
            Err(err) => {
                warn!(error = %err, delivery_id, "Failed to open automation trigger run store");
                continue;
            }
        };
        if let Err(err) = trigger_runs
            .close_github_issue_cycle(&cycle_key(&automation, &trigger, &event))
            .await
        {
            warn!(error = %err, delivery_id, "Failed to close GitHub issue automation trigger cycle");
        }
    }
}

fn matching_github_issue_triggers(
    automations: &[Automation],
    event: &GithubIssueWebhookEvent,
) -> Vec<(Automation, GithubIssueTrigger)> {
    let Some(changed_label) = event.added_label_name() else {
        return Vec::new();
    };
    let issue_labels = event.issue_label_names();
    let mut matches = Vec::new();
    for automation in automations {
        if automation.target.repository != event.repository.full_name {
            continue;
        }
        for trigger in &automation.triggers {
            let AutomationTrigger::GithubIssue(trigger) = trigger else {
                continue;
            };
            if trigger.enabled
                && trigger.trigger_label == changed_label
                && trigger
                    .issue_label
                    .as_ref()
                    .is_none_or(|label| issue_labels.contains(label))
            {
                matches.push((automation.clone(), trigger.clone()));
            }
        }
    }
    matches
}

fn cycle_key(
    automation: &Automation,
    trigger: &GithubIssueTrigger,
    event: &GithubIssueWebhookEvent,
) -> GithubIssueTriggerCycleKey {
    GithubIssueTriggerCycleKey {
        automation_id: automation.id.clone(),
        trigger_id:    trigger.id.clone(),
        repository:    event.repository.full_name.clone(),
        issue_number:  event.issue.number,
    }
}

fn trigger_event_id(
    delivery_id: &str,
    automation: &Automation,
    trigger: &GithubIssueTrigger,
) -> String {
    format!("{delivery_id}:{}:{}", automation.id, trigger.id)
}

async fn record_trigger_run(
    state: &AppState,
    automation: &Automation,
    trigger: &GithubIssueTrigger,
    event: &GithubIssueWebhookEvent,
    delivery_id: &str,
    run_id: Option<RunId>,
    status: AutomationTriggerRunStatus,
) {
    let record = AutomationTriggerRunRecord {
        automation_id: automation.id.clone(),
        trigger_id: trigger.id.clone(),
        trigger_type: AutomationTriggerKind::GithubIssue,
        event_source: AutomationEventSource::Github,
        event_id: delivery_id.to_string(),
        run_id,
        status,
        created_at: Utc::now(),
        closed_at: None,
        context: AutomationTriggerRunContext::GithubIssue {
            repository:    event.repository.full_name.clone(),
            issue_number:  event.issue.number,
            trigger_label: trigger.trigger_label.clone(),
            issue_label:   trigger.issue_label.clone(),
        },
    };
    let trigger_runs = match state.store_ref().automation_trigger_runs().await {
        Ok(trigger_runs) => trigger_runs,
        Err(err) => {
            warn!(error = %err, "Failed to open automation trigger run store");
            return;
        }
    };
    let result = match status {
        AutomationTriggerRunStatus::Started => trigger_runs.record_started(record).await,
        AutomationTriggerRunStatus::FailedToStart => {
            trigger_runs.record_failed_to_start(record).await
        }
    };
    if let Err(err) = result {
        warn!(error = %err, "Failed to record GitHub issue automation trigger run");
    }
}

fn issue_inputs_to_toml(inputs: &GithubIssueRunInputs) -> HashMap<String, toml::Value> {
    HashMap::from([
        (
            "github_issue_url".to_string(),
            toml::Value::String(inputs.github_issue_url.clone()),
        ),
        (
            "github_issue_number".to_string(),
            toml::Value::Integer(inputs.github_issue_number as i64),
        ),
        (
            "github_issue_title".to_string(),
            toml::Value::String(inputs.github_issue_title.clone()),
        ),
        (
            "github_issue_body".to_string(),
            toml::Value::String(inputs.github_issue_body.clone()),
        ),
        (
            "github_issue_author".to_string(),
            toml::Value::String(inputs.github_issue_author.clone()),
        ),
        (
            "github_repository".to_string(),
            toml::Value::String(inputs.github_repository.clone()),
        ),
        (
            "github_default_branch".to_string(),
            toml::Value::String(inputs.github_default_branch.clone()),
        ),
        (
            "github_trigger_label".to_string(),
            toml::Value::String(inputs.github_trigger_label.clone()),
        ),
        (
            "github_delivery_id".to_string(),
            toml::Value::String(inputs.github_delivery_id.clone()),
        ),
    ])
}

async fn github_token(state: &AppState, event: &GithubIssueWebhookEvent) -> anyhow::Result<String> {
    let (owner, repo) = event
        .owner_repo()
        .ok_or_else(|| anyhow::anyhow!("invalid GitHub repository name"))?;
    let settings = state.server_settings();
    let credentials = state
        .github_credentials(&settings.server.integrations.github)
        .map_err(anyhow::Error::msg)?
        .ok_or_else(|| anyhow::anyhow!("GitHub credentials are not configured"))?;
    let client = fabro_http::http_client()?;
    credentials
        .resolve_bearer_token(
            &client,
            owner,
            repo,
            state.github_api_base_url.as_str(),
            serde_json::json!({ "issues": "write" }),
        )
        .await
}

async fn post_success_comment(
    state: &AppState,
    token: Option<&str>,
    event: &GithubIssueWebhookEvent,
    automation_name: &str,
    run_id: RunId,
    run_url: Option<&str>,
) {
    let run_url = run_url.unwrap_or("");
    post_issue_comment(
        state,
        token,
        event,
        &format!("Fabro started automation {automation_name} run {run_id}: {run_url}"),
    )
    .await;
}

async fn post_failure_comment(
    state: &AppState,
    token: Option<&str>,
    event: &GithubIssueWebhookEvent,
    body: String,
) {
    post_issue_comment(state, token, event, &body).await;
}

fn failure_comment(automation_name: &str, reason: &str) -> String {
    format!("Fabro could not start automation {automation_name}: {reason}")
}

fn fire_automation_run_failure_reason(err: &FireAutomationRunError) -> &'static str {
    match err {
        FireAutomationRunError::Materialize(AutomationRunMaterializeError::WorkflowNotFound(_)) => {
            "selected workflow missing or unreadable"
        }
        FireAutomationRunError::Materialize(AutomationRunMaterializeError::Manifest(_)) => {
            "selected workflow invalid"
        }
        FireAutomationRunError::Materialize(
            AutomationRunMaterializeError::InvalidTarget(_)
            | AutomationRunMaterializeError::CloneFailed(_),
        )
        | FireAutomationRunError::Create(_) => "run creation failed",
    }
}

async fn post_issue_comment(
    state: &AppState,
    token: Option<&str>,
    event: &GithubIssueWebhookEvent,
    body: &str,
) {
    let Some(token) = token else {
        warn!("GitHub issue automation comment requested but no token is configured");
        return;
    };
    let Some((owner, repo)) = event.owner_repo() else {
        warn!(repository = %event.repository.full_name, "Cannot post GitHub issue automation comment for invalid repository name");
        return;
    };
    let Ok(client) = fabro_http::http_client() else {
        warn!("Failed to build HTTP client for GitHub issue automation comment");
        return;
    };
    if let Err(err) = fabro_github::create_issue_comment(
        &client,
        token,
        owner,
        repo,
        event.issue.number,
        body,
        state.github_api_base_url.as_str(),
    )
    .await
    {
        warn!(error = %err, issue_number = event.issue.number, repo = %event.repository.full_name, "Failed to post GitHub issue automation comment");
    }
}
