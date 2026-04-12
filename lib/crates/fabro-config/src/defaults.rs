use std::sync::LazyLock;

use fabro_types::settings::SettingsLayer;

use crate::merge::combine_files;
use crate::parse_settings_layer;

static DEFAULTS_LAYER: LazyLock<SettingsLayer> = LazyLock::new(|| {
    parse_settings_layer(include_str!("defaults.toml"))
        .expect("embedded defaults.toml must parse as a valid SettingsLayer")
});

#[must_use]
pub fn defaults_layer() -> &'static SettingsLayer {
    &DEFAULTS_LAYER
}

#[must_use]
pub fn apply_builtin_defaults(layer: SettingsLayer) -> SettingsLayer {
    combine_files(defaults_layer().clone(), layer)
}
