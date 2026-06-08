use chrono::{DateTime, Utc};
use fabro_automation::{AutomationId, AutomationTriggerId};
use fabro_types::RunId;
use futures::StreamExt as _;
use object_store::path::Path as ObjectPath;
use object_store::{Error as ObjectStoreError, PutMode, PutOptions};
use percent_encoding::{AsciiSet, CONTROLS, utf8_percent_encode};
use serde::{Deserialize, Serialize};

use super::TriggerStartDecision;
use crate::{Error, Result};

const ENCODE_SET: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'!')
    .add(b'"')
    .add(b'#')
    .add(b'$')
    .add(b'%')
    .add(b'&')
    .add(b'\'')
    .add(b'(')
    .add(b')')
    .add(b'*')
    .add(b'+')
    .add(b',')
    .add(b'/')
    .add(b':')
    .add(b';')
    .add(b'<')
    .add(b'=')
    .add(b'>')
    .add(b'?')
    .add(b'@')
    .add(b'[')
    .add(b']')
    .add(b'`')
    .add(b'{')
    .add(b'}');

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutomationTriggerRunRecord {
    pub automation_id: AutomationId,
    pub trigger_id:    AutomationTriggerId,
    pub trigger_type:  AutomationTriggerKind,
    pub event_source:  AutomationEventSource,
    pub event_id:      String,
    pub run_id:        Option<RunId>,
    pub status:        AutomationTriggerRunStatus,
    pub created_at:    DateTime<Utc>,
    pub closed_at:     Option<DateTime<Utc>>,
    pub context:       AutomationTriggerRunContext,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutomationTriggerKind {
    GithubIssue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutomationEventSource {
    Github,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutomationTriggerRunStatus {
    Started,
    FailedToStart,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AutomationTriggerRunContext {
    GithubIssue {
        repository:    String,
        issue_number:  u64,
        trigger_label: String,
        issue_label:   Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GithubIssueTriggerCycleKey {
    pub automation_id: AutomationId,
    pub trigger_id:    AutomationTriggerId,
    pub repository:    String,
    pub issue_number:  u64,
}

#[derive(Clone, Debug)]
pub struct AutomationTriggerRunStore {
    pub(super) db: super::Database,
}

impl AutomationTriggerRunStore {
    pub async fn decide_github_issue_start(
        &self,
        key: &GithubIssueTriggerCycleKey,
        delivery_id: &str,
    ) -> Result<TriggerStartDecision> {
        if !self.create_delivery_marker(delivery_id).await? {
            return Ok(TriggerStartDecision::DuplicateDelivery);
        }

        if let Some(existing) = self.read_open_cycle_marker(key).await? {
            self.delete_delivery_marker(delivery_id).await?;
            return Ok(TriggerStartDecision::AlreadyOpen {
                run_id: existing.run_id,
            });
        }

        let trigger_cycle = self.next_github_issue_cycle(key).await?;
        let marker = OpenGithubIssueCycleMarker {
            automation_id: key.automation_id.clone(),
            trigger_id: key.trigger_id.clone(),
            repository: key.repository.clone(),
            issue_number: key.issue_number,
            event_id: delivery_id.to_owned(),
            trigger_cycle,
            run_id: None,
            created_at: Utc::now(),
        };
        let record = AutomationTriggerRunRecord {
            automation_id: key.automation_id.clone(),
            trigger_id:    key.trigger_id.clone(),
            trigger_type:  AutomationTriggerKind::GithubIssue,
            event_source:  AutomationEventSource::Github,
            event_id:      delivery_id.to_owned(),
            run_id:        None,
            status:        AutomationTriggerRunStatus::FailedToStart,
            created_at:    marker.created_at,
            closed_at:     None,
            context:       AutomationTriggerRunContext::GithubIssue {
                repository:    key.repository.clone(),
                issue_number:  key.issue_number,
                trigger_label: key.trigger_id.to_string(),
                issue_label:   None,
            },
        };
        self.write_record(&record).await?;

        match self.create_open_cycle_marker(&marker).await? {
            CreateMarkerOutcome::Created => Ok(TriggerStartDecision::Start { trigger_cycle }),
            CreateMarkerOutcome::AlreadyExists(existing) => {
                self.delete_record(&record).await?;
                self.delete_delivery_marker(delivery_id).await?;
                Ok(TriggerStartDecision::AlreadyOpen {
                    run_id: existing.and_then(|marker| marker.run_id),
                })
            }
        }
    }

    pub async fn record_started(&self, record: AutomationTriggerRunRecord) -> Result<()> {
        let AutomationTriggerRunContext::GithubIssue {
            repository,
            issue_number,
            ..
        } = &record.context;
        let key = GithubIssueTriggerCycleKey {
            automation_id: record.automation_id.clone(),
            trigger_id:    record.trigger_id.clone(),
            repository:    repository.clone(),
            issue_number:  *issue_number,
        };
        let Some(mut marker) = self.read_open_cycle_marker(&key).await? else {
            return Err(Error::Other(format!(
                "automation trigger cycle is not open for automation {} trigger {} GitHub issue {}/{}",
                record.automation_id, record.trigger_id, repository, issue_number
            )));
        };
        if marker.event_id != record.event_id {
            return Err(Error::Other(format!(
                "automation trigger record event {} does not match open cycle event {}",
                record.event_id, marker.event_id
            )));
        }
        marker.run_id = record.run_id;
        self.write_open_cycle_marker(&marker).await?;
        self.write_record(&record).await
    }

    pub async fn record_failed_to_start(&self, record: AutomationTriggerRunRecord) -> Result<()> {
        self.write_record(&record).await
    }

    pub async fn close_github_issue_cycle(&self, key: &GithubIssueTriggerCycleKey) -> Result<bool> {
        let Some(marker) = self.read_open_cycle_marker(key).await? else {
            return Ok(false);
        };

        if let Some(mut record) = self.read_record(&marker).await? {
            record.closed_at = Some(Utc::now());
            self.write_record(&record).await?;
        }
        self.delete_open_cycle_marker(key).await?;
        Ok(true)
    }

    pub async fn records(&self) -> Result<Vec<AutomationTriggerRunRecord>> {
        self.scan_records().await
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OpenGithubIssueCycleMarker {
    automation_id: AutomationId,
    trigger_id:    AutomationTriggerId,
    repository:    String,
    issue_number:  u64,
    event_id:      String,
    trigger_cycle: u64,
    run_id:        Option<RunId>,
    created_at:    DateTime<Utc>,
}

enum CreateMarkerOutcome {
    Created,
    AlreadyExists(Option<OpenGithubIssueCycleMarker>),
}

fn encoded(value: &str) -> String {
    utf8_percent_encode(value, ENCODE_SET).to_string()
}

fn marker_path(base_prefix: &str, key: &GithubIssueTriggerCycleKey) -> ObjectPath {
    let base_prefix = base_prefix.trim_end_matches('/');
    ObjectPath::from(format!(
        "{base_prefix}/automation_trigger_runs/open/{}/{}/github_issue/{}/{}.json",
        encoded(key.automation_id.as_str()),
        encoded(key.trigger_id.as_str()),
        encoded(&key.repository),
        key.issue_number,
    ))
}

fn delivery_marker_path(base_prefix: &str, delivery_id: &str) -> ObjectPath {
    let base_prefix = base_prefix.trim_end_matches('/');
    ObjectPath::from(format!(
        "{base_prefix}/automation_trigger_runs/deliveries/github/{}.marker",
        encoded(delivery_id),
    ))
}

fn record_path(
    base_prefix: &str,
    automation_id: &AutomationId,
    trigger_id: &AutomationTriggerId,
    event_id: &str,
) -> ObjectPath {
    let base_prefix = base_prefix.trim_end_matches('/');
    ObjectPath::from(format!(
        "{base_prefix}/automation_trigger_runs/records/{}/{}/{}.json",
        encoded(automation_id.as_str()),
        encoded(trigger_id.as_str()),
        encoded(event_id),
    ))
}

fn records_prefix(base_prefix: &str) -> ObjectPath {
    let base_prefix = base_prefix.trim_end_matches('/');
    ObjectPath::from(format!("{base_prefix}/automation_trigger_runs/records"))
}

impl AutomationTriggerRunStore {
    async fn create_open_cycle_marker(
        &self,
        marker: &OpenGithubIssueCycleMarker,
    ) -> Result<CreateMarkerOutcome> {
        let key = GithubIssueTriggerCycleKey {
            automation_id: marker.automation_id.clone(),
            trigger_id:    marker.trigger_id.clone(),
            repository:    marker.repository.clone(),
            issue_number:  marker.issue_number,
        };
        let path = marker_path(&self.db.base_prefix, &key);
        let bytes = serde_json::to_vec(marker)?;
        let result = self
            .db
            .object_store
            .put_opts(&path, bytes.into(), PutOptions {
                mode: PutMode::Create,
                ..PutOptions::default()
            })
            .await;

        match result {
            Ok(_) => Ok(CreateMarkerOutcome::Created),
            Err(ObjectStoreError::AlreadyExists { .. }) => Ok(CreateMarkerOutcome::AlreadyExists(
                self.read_open_cycle_marker(&key).await?,
            )),
            Err(err) => Err(Error::Other(format!(
                "failed to create automation trigger run marker {}: {err}",
                path
            ))),
        }
    }

    async fn write_open_cycle_marker(&self, marker: &OpenGithubIssueCycleMarker) -> Result<()> {
        let key = GithubIssueTriggerCycleKey {
            automation_id: marker.automation_id.clone(),
            trigger_id:    marker.trigger_id.clone(),
            repository:    marker.repository.clone(),
            issue_number:  marker.issue_number,
        };
        let path = marker_path(&self.db.base_prefix, &key);
        let bytes = serde_json::to_vec(marker)?;
        self.db.object_store.put(&path, bytes.into()).await?;
        Ok(())
    }

    async fn read_open_cycle_marker(
        &self,
        key: &GithubIssueTriggerCycleKey,
    ) -> Result<Option<OpenGithubIssueCycleMarker>> {
        let path = marker_path(&self.db.base_prefix, key);
        match self.db.object_store.get(&path).await {
            Ok(result) => {
                let bytes = result.bytes().await?;
                Ok(Some(serde_json::from_slice(&bytes)?))
            }
            Err(ObjectStoreError::NotFound { .. }) => Ok(None),
            Err(err) => Err(Error::Other(format!(
                "failed to read automation trigger run marker {}: {err}",
                path
            ))),
        }
    }

    async fn delete_open_cycle_marker(&self, key: &GithubIssueTriggerCycleKey) -> Result<()> {
        let path = marker_path(&self.db.base_prefix, key);
        match self.db.object_store.delete(&path).await {
            Ok(()) | Err(ObjectStoreError::NotFound { .. }) => Ok(()),
            Err(err) => Err(Error::Other(format!(
                "failed to delete automation trigger run marker {}: {err}",
                path
            ))),
        }
    }

    async fn create_delivery_marker(&self, delivery_id: &str) -> Result<bool> {
        let path = delivery_marker_path(&self.db.base_prefix, delivery_id);
        let result = self
            .db
            .object_store
            .put_opts(&path, bytes::Bytes::from_static(b"1").into(), PutOptions {
                mode: PutMode::Create,
                ..PutOptions::default()
            })
            .await;
        match result {
            Ok(_) => Ok(true),
            Err(ObjectStoreError::AlreadyExists { .. }) => Ok(false),
            Err(err) => Err(Error::Other(format!(
                "failed to create automation trigger delivery marker {}: {err}",
                path
            ))),
        }
    }

    async fn delete_delivery_marker(&self, delivery_id: &str) -> Result<()> {
        let path = delivery_marker_path(&self.db.base_prefix, delivery_id);
        match self.db.object_store.delete(&path).await {
            Ok(()) | Err(ObjectStoreError::NotFound { .. }) => Ok(()),
            Err(err) => Err(Error::Other(format!(
                "failed to delete automation trigger delivery marker {}: {err}",
                path
            ))),
        }
    }

    async fn write_record(&self, record: &AutomationTriggerRunRecord) -> Result<()> {
        let path = record_path(
            &self.db.base_prefix,
            &record.automation_id,
            &record.trigger_id,
            &record.event_id,
        );
        let bytes = serde_json::to_vec(record)?;
        self.db.object_store.put(&path, bytes.into()).await?;
        Ok(())
    }

    async fn delete_record(&self, record: &AutomationTriggerRunRecord) -> Result<()> {
        let path = record_path(
            &self.db.base_prefix,
            &record.automation_id,
            &record.trigger_id,
            &record.event_id,
        );
        match self.db.object_store.delete(&path).await {
            Ok(()) | Err(ObjectStoreError::NotFound { .. }) => Ok(()),
            Err(err) => Err(Error::Other(format!(
                "failed to delete automation trigger run record {}: {err}",
                path
            ))),
        }
    }

    async fn read_record(
        &self,
        marker: &OpenGithubIssueCycleMarker,
    ) -> Result<Option<AutomationTriggerRunRecord>> {
        let path = record_path(
            &self.db.base_prefix,
            &marker.automation_id,
            &marker.trigger_id,
            &marker.event_id,
        );
        match self.db.object_store.get(&path).await {
            Ok(result) => {
                let bytes = result.bytes().await?;
                Ok(Some(serde_json::from_slice(&bytes)?))
            }
            Err(ObjectStoreError::NotFound { .. }) => Ok(None),
            Err(err) => Err(Error::Other(format!(
                "failed to read automation trigger run record {}: {err}",
                path
            ))),
        }
    }

    async fn scan_records(&self) -> Result<Vec<AutomationTriggerRunRecord>> {
        let prefix = records_prefix(&self.db.base_prefix);
        let mut stream = self.db.object_store.list(Some(&prefix));
        let mut records = Vec::new();
        while let Some(result) = stream.next().await {
            let object = result.map_err(|err| {
                Error::Other(format!(
                    "failed to list automation trigger run records {}: {err}",
                    prefix
                ))
            })?;
            let bytes = self
                .db
                .object_store
                .get(&object.location)
                .await?
                .bytes()
                .await?;
            records.push(serde_json::from_slice(&bytes)?);
        }
        records.sort_by(|left: &AutomationTriggerRunRecord, right| {
            left.automation_id
                .cmp(&right.automation_id)
                .then_with(|| left.trigger_id.cmp(&right.trigger_id))
                .then_with(|| left.event_id.cmp(&right.event_id))
        });
        Ok(records)
    }

    async fn next_github_issue_cycle(&self, key: &GithubIssueTriggerCycleKey) -> Result<u64> {
        let mut matching_records = 0_u64;
        for record in self.scan_records().await? {
            let AutomationTriggerRunContext::GithubIssue {
                repository,
                issue_number,
                ..
            } = record.context;
            if record.automation_id == key.automation_id
                && record.trigger_id == key.trigger_id
                && repository == key.repository
                && issue_number == key.issue_number
            {
                matching_records += 1;
            }
        }
        Ok(matching_records + 1)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use fabro_automation::{AutomationId, AutomationTriggerId};
    use fabro_types::fixtures::RUN_1;
    use object_store::memory::InMemory;

    use super::*;
    use crate::slate::Database;

    fn database(object_store: Arc<InMemory>) -> Database {
        Database::new(
            object_store,
            "automation-trigger-run-tests",
            Duration::from_millis(1),
            None,
        )
    }

    fn key() -> GithubIssueTriggerCycleKey {
        GithubIssueTriggerCycleKey {
            automation_id: AutomationId::new("issue-bug").unwrap(),
            trigger_id:    AutomationTriggerId::new("bug").unwrap(),
            repository:    "owner/repo".to_string(),
            issue_number:  123,
        }
    }

    fn started_record(
        key: &GithubIssueTriggerCycleKey,
        event_id: &str,
    ) -> AutomationTriggerRunRecord {
        AutomationTriggerRunRecord {
            automation_id: key.automation_id.clone(),
            trigger_id:    key.trigger_id.clone(),
            trigger_type:  AutomationTriggerKind::GithubIssue,
            event_source:  AutomationEventSource::Github,
            event_id:      event_id.to_owned(),
            run_id:        Some(RUN_1),
            status:        AutomationTriggerRunStatus::Started,
            created_at:    Utc::now(),
            closed_at:     None,
            context:       AutomationTriggerRunContext::GithubIssue {
                repository:    key.repository.clone(),
                issue_number:  key.issue_number,
                trigger_label: key.trigger_id.to_string(),
                issue_label:   Some("bug".to_string()),
            },
        }
    }

    #[tokio::test]
    async fn github_issue_cycle_blocks_until_closed_and_tracks_delivery() {
        let store = database(Arc::new(InMemory::new()))
            .automation_trigger_runs()
            .await
            .unwrap();
        let key = key();

        assert_eq!(
            store
                .decide_github_issue_start(&key, "delivery-1")
                .await
                .unwrap(),
            TriggerStartDecision::Start { trigger_cycle: 1 }
        );
        assert_eq!(
            store
                .decide_github_issue_start(&key, "delivery-1")
                .await
                .unwrap(),
            TriggerStartDecision::DuplicateDelivery
        );
        assert_eq!(
            store
                .decide_github_issue_start(&key, "delivery-2")
                .await
                .unwrap(),
            TriggerStartDecision::AlreadyOpen { run_id: None }
        );
        let records = store.records().await.unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].event_id, "delivery-1");
        assert!(store.close_github_issue_cycle(&key).await.unwrap());
        assert_eq!(
            store
                .decide_github_issue_start(&key, "delivery-2")
                .await
                .unwrap(),
            TriggerStartDecision::Start { trigger_cycle: 2 }
        );
    }

    #[tokio::test]
    async fn concurrent_same_delivery_is_duplicate_across_store_instances() {
        let object_store = Arc::new(InMemory::new());
        let store_a = database(Arc::clone(&object_store))
            .automation_trigger_runs()
            .await
            .unwrap();
        let store_b = database(object_store)
            .automation_trigger_runs()
            .await
            .unwrap();
        let key = key();

        let (left, right) = tokio::join!(
            store_a.decide_github_issue_start(&key, "delivery-1"),
            store_b.decide_github_issue_start(&key, "delivery-1"),
        );
        let mut decisions = vec![left.unwrap(), right.unwrap()];
        decisions.sort_by_key(|decision| match decision {
            TriggerStartDecision::Start { .. } => 0,
            TriggerStartDecision::DuplicateDelivery => 1,
            TriggerStartDecision::AlreadyOpen { .. } => 2,
        });

        assert_eq!(decisions, vec![
            TriggerStartDecision::Start { trigger_cycle: 1 },
            TriggerStartDecision::DuplicateDelivery,
        ]);
    }

    #[tokio::test]
    async fn record_started_fails_without_prior_open_cycle() {
        let store = database(Arc::new(InMemory::new()))
            .automation_trigger_runs()
            .await
            .unwrap();
        let key = key();

        assert!(
            store
                .record_started(started_record(&key, "delivery-1"))
                .await
                .is_err()
        );
        assert!(store.records().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn record_started_fails_after_cycle_is_closed() {
        let store = database(Arc::new(InMemory::new()))
            .automation_trigger_runs()
            .await
            .unwrap();
        let key = key();

        assert_eq!(
            store
                .decide_github_issue_start(&key, "delivery-1")
                .await
                .unwrap(),
            TriggerStartDecision::Start { trigger_cycle: 1 }
        );
        assert!(store.close_github_issue_cycle(&key).await.unwrap());

        assert!(
            store
                .record_started(started_record(&key, "delivery-1"))
                .await
                .is_err()
        );
        let records = store.records().await.unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].status, AutomationTriggerRunStatus::FailedToStart);
        assert_eq!(records[0].run_id, None);
        assert!(records[0].closed_at.is_some());
    }

    #[tokio::test]
    async fn encoded_paths_persist_records_across_store_instances() {
        let object_store = Arc::new(InMemory::new());
        let store_a = database(Arc::clone(&object_store))
            .automation_trigger_runs()
            .await
            .unwrap();
        let store_b = database(object_store)
            .automation_trigger_runs()
            .await
            .unwrap();
        let key = GithubIssueTriggerCycleKey {
            automation_id: AutomationId::new("issue-bug").unwrap(),
            trigger_id:    AutomationTriggerId::new("bug_1").unwrap(),
            repository:    "owner name/repo#1".to_string(),
            issue_number:  123,
        };

        assert_eq!(
            store_a
                .decide_github_issue_start(&key, "delivery/with spaces#1")
                .await
                .unwrap(),
            TriggerStartDecision::Start { trigger_cycle: 1 }
        );
        store_a
            .record_started(started_record(&key, "delivery/with spaces#1"))
            .await
            .unwrap();

        let records = store_b.records().await.unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].event_id, "delivery/with spaces#1");
        assert_eq!(records[0].run_id, Some(RUN_1));
    }
}
