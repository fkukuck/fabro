use std::sync::LazyLock;

use fabro_types::settings::SettingsLayer;

use crate::parse_settings_layer;

pub(crate) static DEFAULTS_LAYER: LazyLock<SettingsLayer> = LazyLock::new(|| {
    parse_settings_layer(include_str!("defaults.toml"))
        .expect("embedded defaults.toml must parse as a valid SettingsLayer")
});
