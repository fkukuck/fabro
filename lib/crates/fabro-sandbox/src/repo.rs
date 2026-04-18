use std::path::Path;

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
