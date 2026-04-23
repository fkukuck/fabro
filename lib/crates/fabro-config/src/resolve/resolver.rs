//! Cache builtin defaults across multiple per-namespace resolutions.
//!
//! [`resolve_storage_root`] and the per-namespace `resolve_*_from_file`
//! helpers each call [`apply_builtin_defaults`], which clones both the input
//! layer and the embedded defaults layer before merging them. Callers that
//! need more than one namespace would otherwise pay that cost N times.
//!
//! [`Resolver`] applies defaults once on construction, then exposes per-
//! namespace methods that work against the materialized layer. It is the
//! shared backend for the standalone `resolve_*_from_file` helpers and the
//! preferred entrypoint when more than one namespace is needed.

use fabro_types::settings::{
    CliNamespace, FeaturesNamespace, InterpString, ProjectNamespace, RunNamespace, ServerNamespace,
    SettingsLayer, WorkflowNamespace,
};

use super::{
    ResolveError, default_interp, resolve_cli, resolve_features, resolve_project, resolve_run,
    resolve_server, resolve_workflow,
};
use crate::apply_builtin_defaults;
use crate::user::default_storage_dir;

pub struct Resolver {
    layer: SettingsLayer,
}

impl Resolver {
    #[must_use]
    pub fn from_layer(layer: &SettingsLayer) -> Self {
        Self {
            layer: apply_builtin_defaults(layer.clone()),
        }
    }

    pub fn cli(&self) -> Result<CliNamespace, Vec<ResolveError>> {
        let mut errors = Vec::new();
        let value = self.cli_into(&mut errors);
        finish(value, errors)
    }

    pub fn server(&self) -> Result<ServerNamespace, Vec<ResolveError>> {
        let mut errors = Vec::new();
        let value = self.server_into(&mut errors);
        finish(value, errors)
    }

    pub fn project(&self) -> Result<ProjectNamespace, Vec<ResolveError>> {
        let mut errors = Vec::new();
        let value = self.project_into(&mut errors);
        finish(value, errors)
    }

    pub fn features(&self) -> Result<FeaturesNamespace, Vec<ResolveError>> {
        let mut errors = Vec::new();
        let value = self.features_into(&mut errors);
        finish(value, errors)
    }

    pub fn run(&self) -> Result<RunNamespace, Vec<ResolveError>> {
        let mut errors = Vec::new();
        let value = self.run_into(&mut errors);
        finish(value, errors)
    }

    pub fn workflow(&self) -> Result<WorkflowNamespace, Vec<ResolveError>> {
        let mut errors = Vec::new();
        let value = self.workflow_into(&mut errors);
        finish(value, errors)
    }

    /// Resolved storage root, defaulting to [`default_storage_dir`] when the
    /// input layer doesn't pin one.
    #[must_use]
    pub fn storage_root(&self) -> InterpString {
        self.layer
            .server
            .as_ref()
            .and_then(|server| server.storage.as_ref())
            .and_then(|storage| storage.root.clone())
            .unwrap_or_else(|| default_interp(default_storage_dir()))
    }

    pub fn cli_into(&self, errors: &mut Vec<ResolveError>) -> CliNamespace {
        let layer = self.layer.cli.clone().unwrap_or_default();
        resolve_cli(&layer, errors)
    }

    pub fn server_into(&self, errors: &mut Vec<ResolveError>) -> ServerNamespace {
        let layer = self.layer.server.clone().unwrap_or_default();
        resolve_server(&layer, errors)
    }

    pub fn project_into(&self, errors: &mut Vec<ResolveError>) -> ProjectNamespace {
        let layer = self.layer.project.clone().unwrap_or_default();
        resolve_project(&layer, errors)
    }

    pub fn features_into(&self, errors: &mut Vec<ResolveError>) -> FeaturesNamespace {
        let layer = self.layer.features.clone().unwrap_or_default();
        resolve_features(&layer, errors)
    }

    pub fn run_into(&self, errors: &mut Vec<ResolveError>) -> RunNamespace {
        let layer = self.layer.run.clone().unwrap_or_default();
        resolve_run(&layer, errors)
    }

    pub fn workflow_into(&self, errors: &mut Vec<ResolveError>) -> WorkflowNamespace {
        let layer = self.layer.workflow.clone().unwrap_or_default();
        resolve_workflow(&layer, errors)
    }
}

fn finish<T>(value: T, errors: Vec<ResolveError>) -> Result<T, Vec<ResolveError>> {
    if errors.is_empty() {
        Ok(value)
    } else {
        Err(errors)
    }
}
