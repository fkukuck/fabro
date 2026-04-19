use std::path::Path;

#[cfg(any(feature = "daytona", feature = "azure"))]
pub(crate) fn detect_repo_info(path: &Path) -> Result<(String, Option<String>), String> {
    let repo = git2::Repository::discover(path)
        .map_err(|e| format!("Failed to discover git repo at {}: {e}", path.display()))?;

    let url = repo
        .find_remote("origin")
        .map_err(|e| format!("Failed to find 'origin' remote: {e}"))?
        .url()
        .ok_or_else(|| "origin remote URL is not valid UTF-8".to_string())?
        .to_string();

    let branch = repo
        .head()
        .ok()
        .and_then(|head| head.shorthand().map(String::from));

    Ok((url, branch))
}

#[cfg(any(feature = "daytona", feature = "azure"))]
pub(crate) fn resolve_clone_source(
    explicit_origin_url: Option<&str>,
    explicit_branch: Option<&str>,
    fallback_repo_path: &Path,
) -> Result<(String, Option<String>), String> {
    if let Some(origin_url) = explicit_origin_url {
        return Ok((
            origin_url.to_string(),
            explicit_branch.map(std::string::ToString::to_string),
        ));
    }

    let (detected_url, detected_branch) = detect_repo_info(fallback_repo_path)?;
    Ok((
        detected_url,
        explicit_branch
            .map(std::string::ToString::to_string)
            .or(detected_branch),
    ))
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    #[test]
    fn resolve_clone_source_prefers_explicit_origin_over_detected_repo() {
        let source = resolve_clone_source(
            Some("git@github.com:fkukuck/agentic-factory-prisma.git"),
            Some("fabro-software-factory"),
            Path::new("."),
        )
        .unwrap();

        assert_eq!(
            source.0,
            "git@github.com:fkukuck/agentic-factory-prisma.git"
        );
        assert_eq!(source.1.as_deref(), Some("fabro-software-factory"));
    }

    #[test]
    fn resolve_clone_source_falls_back_to_detected_repo_when_explicit_origin_missing() {
        let dir = TempDir::new().unwrap();
        let repo = git2::Repository::init(dir.path()).unwrap();
        repo.remote(
            "origin",
            "git@github.com:fkukuck/agentic-factory-prisma.git",
        )
        .unwrap();
        let source = resolve_clone_source(None, Some("main"), dir.path()).unwrap();

        assert_eq!(
            source.0,
            "git@github.com:fkukuck/agentic-factory-prisma.git"
        );
        assert_eq!(source.1.as_deref(), Some("main"));
    }
}
