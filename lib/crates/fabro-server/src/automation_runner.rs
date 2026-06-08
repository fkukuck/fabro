use std::collections::HashMap;
use std::sync::Arc;

use axum::http::{HeaderMap, StatusCode};
use fabro_automation::{Automation, AutomationTriggerId};
use fabro_types::{AutomationRef, Principal, RunId, RunSourceContext};

use crate::automation_materializer::{
    AutomationRunMaterializeError, AutomationRunMaterializeInput,
};
use crate::error::ApiError;
use crate::server::AppState;
use crate::server::handler::{lifecycle, runs};

#[cfg(any(test, feature = "test-support"))]
pub(crate) type AutomationRunStartOverride =
    Arc<dyn Fn(RunId) -> Result<(), ApiError> + Send + Sync>;

pub(crate) struct FireAutomationRunInput {
    pub(crate) automation:      Automation,
    pub(crate) trigger_id:      AutomationTriggerId,
    pub(crate) actor:           Principal,
    pub(crate) headers:         HeaderMap,
    pub(crate) input_overrides: HashMap<String, toml::Value>,
    pub(crate) title_override:  Option<String>,
    pub(crate) source_context:  Option<RunSourceContext>,
}

pub(crate) struct FiredAutomationRun {
    pub(crate) created:      CreatedRunFromManifest,
    pub(crate) start_result: Result<(), ApiError>,
}

pub(crate) struct CreatedRunFromManifest {
    pub(crate) run_id:  RunId,
    pub(crate) summary: fabro_types::Run,
}

#[derive(Debug)]
pub(crate) enum FireAutomationRunError {
    Materialize(AutomationRunMaterializeError),
    Create(ApiError),
}

impl FireAutomationRunError {
    pub(crate) fn into_api_error(self) -> ApiError {
        match self {
            Self::Materialize(err) => {
                ApiError::new(StatusCode::UNPROCESSABLE_ENTITY, err.to_string())
            }
            Self::Create(err) => err,
        }
    }
}

pub(crate) async fn fire_automation_run(
    state: Arc<AppState>,
    input: FireAutomationRunInput,
) -> Result<FiredAutomationRun, FireAutomationRunError> {
    let FireAutomationRunInput {
        automation,
        trigger_id,
        actor,
        headers,
        input_overrides,
        title_override,
        source_context,
    } = input;
    let run_id = RunId::new();
    let materialized = state
        .materialize_automation_run(AutomationRunMaterializeInput {
            automation_id: automation.id.clone(),
            target: automation.target.clone(),
            run_id,
            user_settings_path: state.active_config_path().to_path_buf(),
            temp_root: state.automation_temp_root(),
            input_overrides,
            title_override,
        })
        .await
        .map_err(FireAutomationRunError::Materialize)?;
    let explicit_title_supplied = materialized.manifest.title.is_some();
    let automation_ref = AutomationRef {
        id:         automation.id.to_string(),
        name:       Some(automation.name.clone()),
        trigger_id: Some(trigger_id.to_string()),
    };

    let response = Box::pin(runs::create_run_from_manifest(
        Arc::clone(&state),
        runs::CreateRunFromManifestRequest {
            manifest: materialized.manifest,
            submitted_manifest_bytes: materialized.submitted_manifest_bytes,
            explicit_run_id: Some(run_id),
            explicit_title_supplied,
            actor: actor.clone(),
            headers,
            automation: Some(automation_ref),
            source_context,
        },
    ))
    .await;
    let status = response.status();
    if !status.is_success() {
        return Err(FireAutomationRunError::Create(ApiError::new(
            status,
            "failed to create automation run",
        )));
    }
    let summary = state
        .store_ref()
        .get_cached_summary(&run_id, chrono::Utc::now())
        .await
        .map_err(|err| {
            FireAutomationRunError::Create(ApiError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                err.to_string(),
            ))
        })?
        .ok_or_else(|| {
            FireAutomationRunError::Create(ApiError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "created automation run summary missing",
            ))
        })?;
    let created = CreatedRunFromManifest { run_id, summary };

    #[cfg(any(test, feature = "test-support"))]
    let start_result = if let Some(start_override) = state.automation_run_start_override.as_ref() {
        start_override(run_id)
    } else {
        lifecycle::queue_run_start(state.as_ref(), run_id, false, actor).await
    };
    #[cfg(not(any(test, feature = "test-support")))]
    let start_result = lifecycle::queue_run_start(state.as_ref(), run_id, false, actor).await;
    Ok(FiredAutomationRun {
        created,
        start_result,
    })
}

#[cfg(test)]
mod tests {
    use axum::http::HeaderMap;
    use fabro_api::types::RunManifest;
    use fabro_automation::{
        ApiTrigger, AutomationId, AutomationRevision, AutomationTarget, AutomationTrigger,
    };
    use fabro_types::{GithubIssueRunSource, SystemActorKind};
    use serde_json::json;

    use super::*;
    use crate::test_support::{TestAppStateBuilder, TestAutomationRunMaterializer};

    fn minimal_manifest() -> RunManifest {
        serde_json::from_value(json!({
            "version": 1,
            "cwd": "/tmp",
            "target": {
                "identifier": "workflow.fabro",
                "path": "workflow.fabro"
            },
            "workflows": {
                "workflow.fabro": {
                    "source": r#"digraph Test {
                        graph [goal="Test"]
                        start [shape=Mdiamond]
                        exit  [shape=Msquare]
                        start -> exit
                    }"#,
                    "files": {}
                }
            }
        }))
        .expect("minimal manifest should deserialize")
    }

    fn test_automation(id: &str, name: &str, trigger_id: AutomationTriggerId) -> Automation {
        Automation {
            id:          AutomationId::new(id).expect("test automation id should be valid"),
            revision:    AutomationRevision::from_bytes(format!("{id}:{name}").as_bytes()),
            name:        name.to_string(),
            description: None,
            target:      AutomationTarget {
                repository:   "fabro-sh/fabro".to_string(),
                ref_selector: "main".to_string(),
                workflow:     "workflow.fabro".to_string(),
            },
            triggers:    vec![AutomationTrigger::Api(ApiTrigger {
                id:      trigger_id,
                enabled: true,
            })],
        }
    }

    #[tokio::test]
    async fn fire_automation_run_creates_started_run_with_automation_ref() {
        let manifest = minimal_manifest();
        let submitted_manifest_bytes = serde_json::to_vec(&manifest).unwrap();
        let materializer =
            TestAutomationRunMaterializer::succeed(manifest, submitted_manifest_bytes);
        let state = TestAppStateBuilder::new()
            .automation_materializer(materializer)
            .build();
        let automation = test_automation(
            "issue-bug",
            "Implement bug",
            AutomationTriggerId::new("bug").unwrap(),
        );
        let source_context = RunSourceContext::GithubIssue(GithubIssueRunSource {
            repository:   "fabro-sh/fabro".to_string(),
            issue_number: 42,
            issue_title:  "Bug report".to_string(),
            issue_url:    "https://github.com/fabro-sh/fabro/issues/42".to_string(),
        });

        let fired = fire_automation_run(Arc::clone(&state), FireAutomationRunInput {
            automation:      automation.clone(),
            trigger_id:      AutomationTriggerId::new("bug").unwrap(),
            actor:           Principal::System {
                system_kind: SystemActorKind::Engine,
            },
            headers:         HeaderMap::new(),
            input_overrides: HashMap::new(),
            title_override:  None,
            source_context:  Some(source_context.clone()),
        })
        .await
        .expect("automation run should fire");

        assert!(fired.start_result.is_ok());
        assert_eq!(fired.created.run_id, fired.created.summary.id);
        assert_eq!(
            fired.created.summary.automation.clone().unwrap().id,
            "issue-bug"
        );
        assert_eq!(
            fired
                .created
                .summary
                .automation
                .unwrap()
                .trigger_id
                .as_deref(),
            Some("bug")
        );
        assert_eq!(fired.created.summary.source_context, Some(source_context));
    }
}
