use anyhow::{Result, anyhow};
use fabro_api::types;
use fabro_config::RunLayer;
use fabro_workflow::Error as WorkflowError;

pub fn validate_manifest(
    manifest_run_defaults: &RunLayer,
    manifest: &types::RunManifest,
) -> Result<types::ValidateResponse> {
    let prepared = crate::run_manifest::prepare_manifest(manifest_run_defaults, manifest)?;
    let validated = crate::run_manifest::validate_prepared_manifest(&prepared)
        .map_err(validation_error_to_anyhow)?;
    Ok(crate::run_manifest::validate_response(
        &prepared, &validated,
    ))
}

fn validation_error_to_anyhow(err: WorkflowError) -> anyhow::Error {
    anyhow!("{err}")
}
