use anyhow::Result as AnyResult;
use fabro_store::{Database, RunProjection, RunProjectionReducer};
use fabro_types::{ForkSourceRef, RunId};

use super::timeline::{ForkTarget, RunTimeline, TimelineEntry, build_timeline};
use crate::error::Error;
use crate::event::{self, Event};
use crate::records::{Checkpoint, RunSpec};

#[derive(Debug, Clone)]
pub struct ForkRunInput {
    pub source_run_id: RunId,
    pub target:        Option<ForkTarget>,
    pub push:          bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedForkTarget {
    pub checkpoint_ordinal: usize,
    pub node_id:            String,
    pub visit:              usize,
}

impl ResolvedForkTarget {
    #[must_use]
    pub fn response_target(&self) -> String {
        format!("@{}", self.checkpoint_ordinal)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForkOutcome {
    pub source_run_id: RunId,
    pub new_run_id:    RunId,
    pub target:        ResolvedForkTarget,
}

pub async fn fork_run(
    store: &Database,
    input: &ForkRunInput,
) -> std::result::Result<ForkOutcome, Error> {
    let source_run_id = input.source_run_id;
    let run_store = store
        .open_run(&source_run_id)
        .await
        .map_err(|err| Error::engine(err.to_string()))?;
    let state = run_store
        .state()
        .await
        .map_err(|err| Error::engine(err.to_string()))?;
    let timeline = build_timeline(&state).map_err(|err| Error::engine(err.to_string()))?;
    let entry = resolve_fork_entry(&timeline, &source_run_id, input.target.as_ref())
        .map_err(|err| Error::Validation(err.to_string()))?;
    let checkpoint_sha = entry.run_commit_sha.clone().ok_or_else(|| {
        Error::Validation(format!(
            "checkpoint @{} has no git_commit_sha; cannot fork",
            entry.ordinal
        ))
    })?;

    validate_source_spec(state.spec.as_ref(), &checkpoint_sha)?;

    let events = run_store
        .list_events()
        .await
        .map_err(|err| Error::engine(err.to_string()))?;
    let historical_events = events
        .into_iter()
        .filter(|event| event.seq <= entry.checkpoint_seq)
        .collect::<Vec<_>>();
    let mut projection = RunProjection::apply_events(&historical_events)
        .map_err(|err| Error::engine(err.to_string()))?;
    let mut run_spec = projection
        .spec
        .clone()
        .ok_or_else(|| Error::engine("source run projection has no spec"))?;

    let new_run_id = RunId::new();
    run_spec.run_id = new_run_id;
    run_spec.fork_source_ref = Some(ForkSourceRef {
        source_run_id,
        checkpoint_sha: checkpoint_sha.clone(),
    });
    projection.spec = Some(run_spec);
    projection.start = None;
    projection.sandbox = None;
    projection.conclusion = None;
    projection.retro = None;
    projection.retro_prompt = None;
    projection.retro_response = None;
    projection.final_patch = None;
    projection.pull_request = None;
    projection.superseded_by = None;
    if let Some(checkpoint) = projection.checkpoint.as_mut() {
        checkpoint.git_commit_sha = Some(checkpoint_sha);
    }

    persist_forked_run(store, &projection).await?;

    Ok(ForkOutcome {
        source_run_id,
        new_run_id,
        target: ResolvedForkTarget {
            checkpoint_ordinal: entry.ordinal,
            node_id:            entry.node_name.clone(),
            visit:              entry.visit,
        },
    })
}

fn validate_source_spec(
    spec: Option<&RunSpec>,
    checkpoint_sha: &str,
) -> std::result::Result<(), Error> {
    let spec = spec.ok_or_else(|| Error::engine("source run projection has no spec"))?;
    if spec.checkpoints_disabled {
        return Err(Error::Validation(
            "source run was created with checkpoints disabled; cannot fork".to_string(),
        ));
    }
    if checkpoint_sha.trim().is_empty() {
        return Err(Error::Validation(
            "target checkpoint has an empty git_commit_sha; cannot fork".to_string(),
        ));
    }
    let Some(origin) = spec.repo_origin_url.as_ref() else {
        return Err(Error::Validation(
            "source run has no repo_origin_url; cannot validate fork origin".to_string(),
        ));
    };
    if fabro_github::normalize_repo_origin_url(origin).is_empty() {
        return Err(Error::Validation(
            "source run has an empty repo_origin_url; cannot validate fork origin".to_string(),
        ));
    }
    Ok(())
}

fn resolve_fork_entry<'a>(
    timeline: &'a RunTimeline,
    source_run_id: &RunId,
    target: Option<&ForkTarget>,
) -> AnyResult<&'a TimelineEntry> {
    match target {
        Some(target) => timeline.resolve(target),
        None => timeline
            .entries
            .last()
            .ok_or_else(|| anyhow::anyhow!("no checkpoints found for run {source_run_id}")),
    }
}

async fn persist_forked_run(
    store: &Database,
    projection: &RunProjection,
) -> std::result::Result<(), Error> {
    let spec = projection
        .spec
        .as_ref()
        .ok_or_else(|| Error::engine("forked run projection has no spec"))?;
    let checkpoint = projection
        .checkpoint
        .as_ref()
        .ok_or_else(|| Error::engine("forked run projection has no checkpoint"))?;

    let run_store = store
        .create_run(&spec.run_id)
        .await
        .map_err(|err| Error::engine(err.to_string()))?;

    event::append_event(&run_store, &spec.run_id, &Event::RunCreated {
        run_id:               spec.run_id,
        settings:             serde_json::to_value(&spec.settings)
            .map_err(|err| Error::engine(err.to_string()))?,
        graph:                serde_json::to_value(&spec.graph)
            .map_err(|err| Error::engine(err.to_string()))?,
        workflow_source:      projection.graph_source.clone(),
        workflow_config:      None,
        labels:               spec.labels.clone().into_iter().collect(),
        run_dir:              String::new(),
        source_directory:     spec.source_directory.clone(),
        repo_origin_url:      spec.repo_origin_url.clone(),
        base_branch:          spec.base_branch.clone(),
        workflow_slug:        spec.workflow_slug.clone(),
        db_prefix:            None,
        provenance:           spec.provenance.clone(),
        manifest_blob:        spec.manifest_blob,
        pre_run_git:          spec.pre_run_git.clone(),
        fork_source_ref:      spec.fork_source_ref.clone(),
        checkpoints_disabled: spec.checkpoints_disabled,
    })
    .await
    .map_err(|err| Error::engine(err.to_string()))?;

    event::append_event(
        &run_store,
        &spec.run_id,
        &checkpoint_completed_event(checkpoint),
    )
    .await
    .map_err(|err| Error::engine(err.to_string()))?;
    event::append_event(&run_store, &spec.run_id, &Event::RunSubmitted {
        definition_blob: spec.definition_blob,
    })
    .await
    .map_err(|err| Error::engine(err.to_string()))
}

fn checkpoint_completed_event(checkpoint: &Checkpoint) -> Event {
    let status = checkpoint
        .node_outcomes
        .get(&checkpoint.current_node)
        .map_or_else(
            || "success".to_string(),
            |outcome| outcome.status.to_string(),
        );

    Event::CheckpointCompleted {
        node_id: checkpoint.current_node.clone(),
        status,
        current_node: checkpoint.current_node.clone(),
        completed_nodes: checkpoint.completed_nodes.clone(),
        node_retries: checkpoint.node_retries.clone().into_iter().collect(),
        context_values: checkpoint.context_values.clone().into_iter().collect(),
        node_outcomes: checkpoint.node_outcomes.clone().into_iter().collect(),
        next_node_id: checkpoint.next_node_id.clone(),
        git_commit_sha: checkpoint.git_commit_sha.clone(),
        loop_failure_signatures: checkpoint
            .loop_failure_signatures
            .iter()
            .map(|(signature, count)| (signature.to_string(), *count))
            .collect(),
        restart_failure_signatures: checkpoint
            .restart_failure_signatures
            .iter()
            .map(|(signature, count)| (signature.to_string(), *count))
            .collect(),
        node_visits: checkpoint.node_visits.clone().into_iter().collect(),
        diff: None,
    }
}
