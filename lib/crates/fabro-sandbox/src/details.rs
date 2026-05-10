use anyhow::Result;
#[cfg(feature = "daytona")]
use anyhow::{Context, anyhow};
use fabro_types::{
    RunId, SandboxDetails, SandboxRecord, SandboxResources, SandboxState, SandboxTimestamps,
};

/// Inspect the sandbox identified by `record` and return provider-neutral
/// details for control-plane display.
///
/// Provider feature flags determine which branches resolve real data:
/// - `local` always returns a minimal record describing the host.
/// - `docker` inspects the managed container through Bollard.
/// - `daytona` reconnects to the SDK sandbox.
#[allow(
    unused_variables,
    reason = "Feature-gated providers consume some parameters only when enabled."
)]
pub async fn sandbox_details(
    record: &SandboxRecord,
    daytona_api_key: Option<String>,
    daytona_organization_id: Option<String>,
    run_id: Option<RunId>,
) -> Result<SandboxDetails> {
    match record.provider.as_str() {
        "local" => Ok(local_details()),
        #[cfg(feature = "docker")]
        "docker" => docker::docker_details(record, run_id).await,
        #[cfg(feature = "daytona")]
        "daytona" => daytona::daytona_details(record, daytona_api_key).await,
        other => Err(anyhow::anyhow!(
            "Sandbox provider '{other}' has no details implementation"
        )),
    }
}

fn local_details() -> SandboxDetails {
    SandboxDetails {
        provider:     "local".to_string(),
        name:         None,
        id:           None,
        state:        SandboxState::Running,
        native_state: None,
        region:       None,
        image:        None,
        resources:    SandboxResources::default(),
        labels:       Default::default(),
        timestamps:   SandboxTimestamps::default(),
    }
}

#[cfg(feature = "docker")]
mod docker {
    use std::collections::BTreeMap;

    use bollard::Docker;
    use bollard::container::InspectContainerOptions;
    use bollard::models::{ContainerInspectResponse, ContainerStateStatusEnum};
    use chrono::{DateTime, Utc};
    use fabro_types::{
        RunId, SandboxDetails, SandboxRecord, SandboxResources, SandboxState, SandboxTimestamps,
    };

    use super::*;

    pub(super) async fn docker_details(
        record: &SandboxRecord,
        _run_id: Option<RunId>,
    ) -> Result<SandboxDetails> {
        let identifier = record
            .identifier
            .as_deref()
            .context("Docker sandbox record missing identifier (container ID/name)")?;
        let docker =
            Docker::connect_with_local_defaults().context("Failed to connect to Docker daemon")?;
        let inspect = docker
            .inspect_container(identifier, None::<InspectContainerOptions>)
            .await
            .map_err(|err| anyhow!("Failed to inspect Docker container '{identifier}': {err}"))?;
        Ok(map_docker_inspect(inspect))
    }

    fn map_docker_inspect(inspect: ContainerInspectResponse) -> SandboxDetails {
        let status_enum = inspect
            .state
            .as_ref()
            .and_then(|state| state.status.as_ref())
            .copied();
        let normalized_state = status_enum
            .map(normalize_docker_state)
            .unwrap_or(SandboxState::Unknown);
        let native_state = status_enum
            .map(|status| status.to_string())
            .filter(|value| !value.is_empty());

        let host_config = inspect.host_config.as_ref();
        let resources = SandboxResources {
            cpu_cores:    host_config.and_then(docker_cpu_cores),
            memory_bytes: host_config
                .and_then(|host| host.memory)
                .filter(|bytes| *bytes > 0)
                .and_then(|bytes| u64::try_from(bytes).ok()),
            disk_bytes:   None,
        };

        let labels: BTreeMap<String, String> = inspect
            .config
            .and_then(|config| config.labels)
            .map(|map| map.into_iter().collect())
            .unwrap_or_default();

        let name = inspect
            .name
            .map(|raw| raw.trim_start_matches('/').to_string())
            .filter(|name| !name.is_empty());

        let image = inspect.image;

        let created_at = inspect.created.as_deref().and_then(parse_docker_timestamp);

        SandboxDetails {
            provider: "docker".to_string(),
            name,
            id: inspect.id,
            state: normalized_state,
            native_state,
            region: None,
            image,
            resources,
            labels,
            timestamps: SandboxTimestamps {
                created_at,
                last_activity_at: None,
            },
        }
    }

    fn parse_docker_timestamp(value: &str) -> Option<DateTime<Utc>> {
        DateTime::parse_from_rfc3339(value)
            .ok()
            .map(|dt| dt.with_timezone(&Utc))
    }

    pub(super) fn docker_cpu_cores(host_config: &bollard::models::HostConfig) -> Option<f64> {
        let quota = host_config.cpu_quota?;
        let period = host_config.cpu_period?;
        if quota <= 0 || period <= 0 {
            return None;
        }
        #[allow(
            clippy::cast_precision_loss,
            reason = "CPU quota/period are bounded well within f64 mantissa precision."
        )]
        let cores = (quota as f64) / (period as f64);
        Some(cores)
    }

    pub(super) fn normalize_docker_state(status: ContainerStateStatusEnum) -> SandboxState {
        match status {
            ContainerStateStatusEnum::EMPTY => SandboxState::Unknown,
            ContainerStateStatusEnum::CREATED => SandboxState::Provisioning,
            ContainerStateStatusEnum::RUNNING => SandboxState::Running,
            ContainerStateStatusEnum::PAUSED => SandboxState::Paused,
            ContainerStateStatusEnum::RESTARTING => SandboxState::Starting,
            ContainerStateStatusEnum::REMOVING => SandboxState::Deleting,
            ContainerStateStatusEnum::EXITED => SandboxState::Stopped,
            ContainerStateStatusEnum::DEAD => SandboxState::Error,
        }
    }

    #[cfg(test)]
    mod tests {
        use bollard::models::HostConfig;

        use super::*;

        #[test]
        fn cpu_cores_divides_quota_by_period() {
            let host = HostConfig {
                cpu_quota: Some(200_000),
                cpu_period: Some(100_000),
                ..Default::default()
            };
            assert_eq!(docker_cpu_cores(&host), Some(2.0));
        }

        #[test]
        fn cpu_cores_returns_none_when_quota_missing() {
            let host = HostConfig {
                cpu_quota: None,
                cpu_period: Some(100_000),
                ..Default::default()
            };
            assert_eq!(docker_cpu_cores(&host), None);
        }

        #[test]
        fn cpu_cores_returns_none_when_period_zero() {
            let host = HostConfig {
                cpu_quota: Some(200_000),
                cpu_period: Some(0),
                ..Default::default()
            };
            assert_eq!(docker_cpu_cores(&host), None);
        }

        #[test]
        fn memory_bytes_zero_is_unset() {
            let inspect = ContainerInspectResponse {
                host_config: Some(HostConfig {
                    memory: Some(0),
                    ..Default::default()
                }),
                ..Default::default()
            };
            let details = map_docker_inspect(inspect);
            assert_eq!(details.resources.memory_bytes, None);
        }

        #[test]
        fn memory_bytes_present_is_carried_through() {
            let inspect = ContainerInspectResponse {
                host_config: Some(HostConfig {
                    memory: Some(2 * 1024 * 1024 * 1024),
                    ..Default::default()
                }),
                ..Default::default()
            };
            let details = map_docker_inspect(inspect);
            assert_eq!(details.resources.memory_bytes, Some(2_147_483_648));
        }

        #[test]
        fn name_strips_leading_slash() {
            let inspect = ContainerInspectResponse {
                name: Some("/fabro-run-abc".to_string()),
                ..Default::default()
            };
            let details = map_docker_inspect(inspect);
            assert_eq!(details.name.as_deref(), Some("fabro-run-abc"));
        }

        #[test]
        fn empty_status_is_unknown() {
            assert_eq!(
                normalize_docker_state(ContainerStateStatusEnum::EMPTY),
                SandboxState::Unknown
            );
        }

        #[test]
        fn created_status_is_provisioning() {
            assert_eq!(
                normalize_docker_state(ContainerStateStatusEnum::CREATED),
                SandboxState::Provisioning
            );
        }

        #[test]
        fn running_status_is_running() {
            assert_eq!(
                normalize_docker_state(ContainerStateStatusEnum::RUNNING),
                SandboxState::Running
            );
        }

        #[test]
        fn paused_status_is_paused() {
            assert_eq!(
                normalize_docker_state(ContainerStateStatusEnum::PAUSED),
                SandboxState::Paused
            );
        }

        #[test]
        fn restarting_status_is_starting() {
            assert_eq!(
                normalize_docker_state(ContainerStateStatusEnum::RESTARTING),
                SandboxState::Starting
            );
        }

        #[test]
        fn removing_status_is_deleting() {
            assert_eq!(
                normalize_docker_state(ContainerStateStatusEnum::REMOVING),
                SandboxState::Deleting
            );
        }

        #[test]
        fn exited_status_is_stopped() {
            assert_eq!(
                normalize_docker_state(ContainerStateStatusEnum::EXITED),
                SandboxState::Stopped
            );
        }

        #[test]
        fn dead_status_is_error() {
            assert_eq!(
                normalize_docker_state(ContainerStateStatusEnum::DEAD),
                SandboxState::Error
            );
        }

        #[test]
        fn parse_timestamp_accepts_rfc3339() {
            let parsed = parse_docker_timestamp("2026-05-09T12:00:00Z");
            assert!(parsed.is_some());
        }

        #[test]
        fn parse_timestamp_rejects_garbage() {
            assert!(parse_docker_timestamp("not a date").is_none());
        }
    }
}

#[cfg(feature = "daytona")]
mod daytona {
    use std::collections::BTreeMap;

    use chrono::{DateTime, Utc};
    use daytona_api_client::models::SandboxState as DaytonaState;
    use fabro_types::{
        SandboxDetails, SandboxRecord, SandboxResources, SandboxState, SandboxTimestamps,
    };

    use super::*;
    use crate::daytona::DaytonaSandbox;

    pub(super) async fn daytona_details(
        record: &SandboxRecord,
        daytona_api_key: Option<String>,
    ) -> Result<SandboxDetails> {
        let identifier = record
            .identifier
            .as_deref()
            .context("Daytona sandbox record missing identifier (sandbox name)")?;
        let repo_cloned = record
            .repo_cloned
            .context("Daytona sandbox record missing clone metadata")?;

        let sandbox_handle = DaytonaSandbox::reconnect(
            identifier,
            daytona_api_key,
            repo_cloned,
            record.clone_origin_url.clone(),
            record.clone_branch.clone(),
        )
        .await
        .map_err(anyhow::Error::new)?;
        let sdk_sandbox = sandbox_handle
            .sandbox_handle()
            .ok_or_else(|| anyhow!("Daytona sandbox is not initialized after reconnect"))?;

        Ok(map_daytona_sandbox(sdk_sandbox))
    }

    fn map_daytona_sandbox(sandbox: &daytona_sdk::Sandbox) -> SandboxDetails {
        let normalized_state = sandbox
            .state
            .map(normalize_daytona_state)
            .unwrap_or(SandboxState::Unknown);
        let native_state = sandbox.state.map(|state| state.to_string());

        let resources = SandboxResources {
            cpu_cores:    Some(sandbox.cpu),
            memory_bytes: gibibytes_to_bytes(sandbox.memory),
            disk_bytes:   gibibytes_to_bytes(sandbox.disk),
        };

        let labels: BTreeMap<String, String> = sandbox
            .labels
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();

        let target = sandbox.target.clone();
        let region = if target.is_empty() {
            None
        } else {
            Some(target)
        };

        SandboxDetails {
            provider: "daytona".to_string(),
            name: Some(sandbox.name.clone()),
            id: Some(sandbox.id.clone()),
            state: normalized_state,
            native_state,
            region,
            image: sandbox.snapshot.clone(),
            resources,
            labels,
            timestamps: SandboxTimestamps {
                created_at:       sandbox.created_at.as_deref().and_then(parse_iso8601),
                last_activity_at: sandbox.updated_at.as_deref().and_then(parse_iso8601),
            },
        }
    }

    fn parse_iso8601(value: &str) -> Option<DateTime<Utc>> {
        DateTime::parse_from_rfc3339(value)
            .ok()
            .map(|dt| dt.with_timezone(&Utc))
    }

    /// The Daytona SDK reports CPU/memory/disk as floats in their respective
    /// SI units (cores, GiB, GiB). Convert mem/disk into bytes.
    fn gibibytes_to_bytes(value: f64) -> Option<u64> {
        if value <= 0.0 || !value.is_finite() {
            return None;
        }
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            clippy::cast_precision_loss,
            reason = "Daytona memory/disk values are well within u64 range and only need approximate byte counts."
        )]
        let bytes = (value * 1024.0 * 1024.0 * 1024.0) as u64;
        Some(bytes)
    }

    pub(super) fn normalize_daytona_state(state: DaytonaState) -> SandboxState {
        match state {
            DaytonaState::Creating
            | DaytonaState::PendingBuild
            | DaytonaState::BuildingSnapshot
            | DaytonaState::PullingSnapshot => SandboxState::Provisioning,
            DaytonaState::Starting => SandboxState::Starting,
            DaytonaState::Started => SandboxState::Running,
            DaytonaState::Stopping => SandboxState::Stopping,
            DaytonaState::Stopped => SandboxState::Stopped,
            DaytonaState::Restoring => SandboxState::Restoring,
            DaytonaState::Resizing => SandboxState::Resizing,
            DaytonaState::Archiving => SandboxState::Stopping,
            DaytonaState::Archived => SandboxState::Archived,
            DaytonaState::Destroying => SandboxState::Deleting,
            DaytonaState::Destroyed => SandboxState::Deleted,
            DaytonaState::Error | DaytonaState::BuildFailed => SandboxState::Error,
            DaytonaState::Unknown => SandboxState::Unknown,
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn started_normalizes_to_running() {
            assert_eq!(
                normalize_daytona_state(DaytonaState::Started),
                SandboxState::Running
            );
        }

        #[test]
        fn creating_normalizes_to_provisioning() {
            assert_eq!(
                normalize_daytona_state(DaytonaState::Creating),
                SandboxState::Provisioning
            );
        }

        #[test]
        fn building_snapshot_normalizes_to_provisioning() {
            assert_eq!(
                normalize_daytona_state(DaytonaState::BuildingSnapshot),
                SandboxState::Provisioning
            );
        }

        #[test]
        fn stopped_normalizes_to_stopped() {
            assert_eq!(
                normalize_daytona_state(DaytonaState::Stopped),
                SandboxState::Stopped
            );
        }

        #[test]
        fn archived_normalizes_to_archived() {
            assert_eq!(
                normalize_daytona_state(DaytonaState::Archived),
                SandboxState::Archived
            );
        }

        #[test]
        fn destroyed_normalizes_to_deleted() {
            assert_eq!(
                normalize_daytona_state(DaytonaState::Destroyed),
                SandboxState::Deleted
            );
        }

        #[test]
        fn build_failed_normalizes_to_error() {
            assert_eq!(
                normalize_daytona_state(DaytonaState::BuildFailed),
                SandboxState::Error
            );
        }

        #[test]
        fn unknown_normalizes_to_unknown() {
            assert_eq!(
                normalize_daytona_state(DaytonaState::Unknown),
                SandboxState::Unknown
            );
        }

        #[test]
        fn gibibytes_to_bytes_converts_positive_values() {
            assert_eq!(gibibytes_to_bytes(2.0), Some(2 * 1024 * 1024 * 1024));
        }

        #[test]
        fn gibibytes_to_bytes_returns_none_for_zero() {
            assert_eq!(gibibytes_to_bytes(0.0), None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_details_returns_running_with_no_metadata() {
        let details = local_details();
        assert_eq!(details.provider, "local");
        assert_eq!(details.state, SandboxState::Running);
        assert!(details.name.is_none());
        assert!(details.id.is_none());
        assert!(details.region.is_none());
        assert!(details.image.is_none());
        assert!(details.labels.is_empty());
        assert_eq!(details.resources, SandboxResources::default());
        assert_eq!(details.timestamps, SandboxTimestamps::default());
    }
}
