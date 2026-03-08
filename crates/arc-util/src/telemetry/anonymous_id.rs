use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;
use uuid::Uuid;

fn anonymous_id_path() -> Result<PathBuf> {
    let home = dirs::home_dir().context("could not determine home directory")?;
    Ok(home.join(".arc").join("anonymous_id"))
}

pub fn load_or_create_anonymous_id() -> Result<String> {
    let path = anonymous_id_path()?;

    if let Ok(contents) = fs::read_to_string(&path) {
        let id = contents.trim().to_string();
        if !id.is_empty() {
            return Ok(id);
        }
    }

    let id = Uuid::new_v4().to_string();

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create directory {}", parent.display()))?;
    }

    fs::write(&path, &id)
        .with_context(|| format!("failed to write anonymous id to {}", path.display()))?;

    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_or_create_returns_stable_id() {
        // Calls the real function, which uses ~/.arc/anonymous_id
        let id1 = load_or_create_anonymous_id().unwrap();
        let id2 = load_or_create_anonymous_id().unwrap();
        assert_eq!(id1, id2);
        // Should be a valid UUID
        Uuid::parse_str(&id1).unwrap();
    }
}
