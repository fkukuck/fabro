//! Azure Container Apps Sandboxes as a sandbox-driver provider.
//!
//! A sandbox group holds the run's sandboxes; each is a microVM created
//! from a prepared disk image. The provider reaches the group with a
//! service principal's client credentials: Azure Resource Manager for the
//! group itself, and the regional data plane for its sandboxes, commands
//! and files.
//!
//! What ACA does not offer through its REST API, the provider builds in the
//! sandbox or declares unsupported: streamed output, stdin and signals ride
//! on detached commands ([`exec`]); search, git and services are derived
//! from exec; a sandbox port is reached through a private relay
//! ([`ports`]), never public ingress; PTY, SSH and remote desktops are
//! absent from the capability set, so callers see them as unsupported
//! rather than failing late.

// `sandbox_driver::Error` is the contract every provider returns.
#![expect(
    clippy::result_large_err,
    reason = "sandbox-driver's typed error is the provider contract"
)]

mod client;
mod config;
mod exec;
mod fs;
mod models;
mod ports;
mod provider;
mod sandbox;

pub use client::{AcaClient, SandboxGroupInfo};
pub use config::{
    API_VERSION, AcaConnection, AcaEndpoints, AcaPolicy, AcaServicePrincipal, AcaTarget,
    DATA_OWNER_ROLE, DATA_PLANE_SCOPE, MAX_RETENTION_SECS, READER_ROLE,
};
pub use provider::{
    AcaImage, AcaProvider, AcaProviderConfig, CREATE_TIMEOUT, DEFAULT_IMAGE,
    DEFAULT_SUSPEND_AFTER_IDLE_SECS, MAX_CPU_CORES, MAX_DISK_MB, MAX_MEMORY_MB, ReadinessReport,
    WORKSPACE, aca_capabilities, resource_problems,
};

/// The provider kind leases and run records name.
pub const KIND: &str = "aca";

/// Connects a provider for `connection` over `http`, without contacting
/// Azure: the first call authenticates.
#[must_use]
pub fn connect(
    connection: &AcaConnection,
    endpoints: AcaEndpoints,
    http: fabro_http::HttpClient,
) -> AcaProvider {
    let client = AcaClient::new(
        connection.target.clone(),
        connection.credential.clone(),
        endpoints,
        http,
    );
    AcaProvider::new(client, connection.policy.clone())
}
