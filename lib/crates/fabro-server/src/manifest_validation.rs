use anyhow::Result;
use fabro_api::types;
use fabro_config::RunLayer;

use crate::run_manifest;

pub fn validate_manifest(
    manifest_run_defaults: &RunLayer,
    manifest: &types::RunManifest,
) -> Result<types::ValidateResponse> {
    let prepared = run_manifest::prepare_manifest(manifest_run_defaults, manifest)?;
    let validated =
        run_manifest::validate_prepared_manifest(&prepared).map_err(anyhow::Error::new)?;
    Ok(run_manifest::validate_response(&prepared, &validated))
}
