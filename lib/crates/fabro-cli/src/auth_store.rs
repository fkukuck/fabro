#![expect(
    clippy::disallowed_methods,
    reason = "CLI auth storage is local file I/O, not a hot async path."
)]
#![expect(
    clippy::disallowed_types,
    reason = "CLI auth storage requires std::fs::File handles for advisory locking."
)]

use std::collections::BTreeMap;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::{fmt, fs};

use chrono::{DateTime, Utc};
use fs2::FileExt;
use rand::Rng;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::user_config::ServerTarget;

const AUTH_FILE_ENV: &str = "FABRO_AUTH_FILE";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Subject {
    pub(crate) idp_issuer:  String,
    pub(crate) idp_subject: String,
    pub(crate) login:       String,
    pub(crate) name:        String,
    pub(crate) email:       String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct AuthEntry {
    pub(crate) access_token:             String,
    pub(crate) access_token_expires_at:  DateTime<Utc>,
    pub(crate) refresh_token:            String,
    pub(crate) refresh_token_expires_at: DateTime<Utc>,
    pub(crate) subject:                  Subject,
    pub(crate) logged_in_at:             DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct ServerTargetKey(String);

impl ServerTargetKey {
    pub(crate) fn new(target: &ServerTarget) -> Result<Self, AuthStoreError> {
        match target {
            ServerTarget::HttpUrl { api_url, .. } => canonical_http_target(api_url).map(Self),
            ServerTarget::UnixSocket(path) => Ok(Self(format!(
                "unix://{}",
                canonical_socket_path(path)?.display()
            ))),
        }
    }

    fn from_canonical(canonical: String) -> Self {
        Self(canonical)
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ServerTargetKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<&ServerTarget> for ServerTargetKey {
    type Error = AuthStoreError;

    fn try_from(value: &ServerTarget) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

#[derive(Debug, Error)]
pub(crate) enum AuthStoreError {
    #[allow(
        dead_code,
        reason = "This platform-gated variant is exercised on non-Unix targets."
    )]
    #[error("CLI OAuth login is not supported on this platform in this release.")]
    UnsupportedPlatform,
    #[error("invalid server target `{value}`")]
    InvalidServerTarget { value: String },
    #[error(transparent)]
    Lock(#[from] LockError),
    #[error("failed to read auth store at {path}: {source}")]
    Read {
        path:   PathBuf,
        source: std::io::Error,
    },
    #[error("failed to parse auth store at {path}: {source}")]
    Corrupt {
        path:   PathBuf,
        source: serde_json::Error,
    },
    #[error("failed to create auth store directory {path}: {source}")]
    CreateDir {
        path:   PathBuf,
        source: std::io::Error,
    },
    #[error("failed to write auth store at {path}: {source}")]
    Write {
        path:   PathBuf,
        source: std::io::Error,
    },
    #[error("failed to serialize auth store at {path}: {source}")]
    Serialize {
        path:   PathBuf,
        source: serde_json::Error,
    },
}

#[derive(Debug, Error)]
pub(crate) enum LockError {
    #[error(
        "the filesystem backing {path} does not support file locking; move the auth store to a local filesystem or set {AUTH_FILE_ENV} to a local path"
    )]
    FilesystemDoesNotSupportLocking { path: PathBuf },
    #[error("failed to lock auth store at {path}: {source}")]
    Io {
        path:   PathBuf,
        source: std::io::Error,
    },
}

#[derive(Debug, Clone)]
pub(crate) struct AuthStore {
    path: PathBuf,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct AuthFile {
    #[serde(default)]
    servers: BTreeMap<String, AuthEntry>,
}

impl Default for AuthStore {
    fn default() -> Self {
        let path = std::env::var_os(AUTH_FILE_ENV)
            .map(PathBuf::from)
            .unwrap_or_else(|| fabro_util::Home::from_env().root().join("auth.json"));
        Self::new(path)
    }
}

impl AuthStore {
    pub(crate) fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub(crate) fn get(&self, key: &ServerTargetKey) -> Result<Option<AuthEntry>, AuthStoreError> {
        if !self.path.exists() {
            return Ok(None);
        }

        self.with_shared_lock(|| {
            let file = self.read_auth_file()?;
            Ok(file.servers.get(key.as_str()).cloned())
        })
    }

    #[allow(dead_code, reason = "Login wiring lands in a later CLI auth unit.")]
    pub(crate) fn put(
        &self,
        key: &ServerTargetKey,
        entry: AuthEntry,
    ) -> Result<(), AuthStoreError> {
        #[cfg(not(unix))]
        {
            let _ = (key, entry);
            Err(AuthStoreError::UnsupportedPlatform)
        }

        #[cfg(unix)]
        {
            self.ensure_parent_dir()?;
            self.with_exclusive_lock(|| {
                let mut file = self.read_auth_file_if_exists()?;
                file.servers.insert(key.to_string(), entry);
                self.write_auth_file(&file)
            })
        }
    }

    #[allow(dead_code, reason = "Logout wiring lands in a later CLI auth unit.")]
    pub(crate) fn remove(&self, key: &ServerTargetKey) -> Result<bool, AuthStoreError> {
        #[cfg(not(unix))]
        {
            let _ = key;
            Err(AuthStoreError::UnsupportedPlatform)
        }

        #[cfg(unix)]
        {
            if !self.path.exists() {
                return Ok(false);
            }

            self.ensure_parent_dir()?;
            self.with_exclusive_lock(|| {
                let mut file = self.read_auth_file_if_exists()?;
                let removed = file.servers.remove(key.as_str()).is_some();
                self.write_auth_file(&file)?;
                Ok(removed)
            })
        }
    }

    pub(crate) fn list(&self) -> Result<Vec<(ServerTargetKey, AuthEntry)>, AuthStoreError> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }

        self.with_shared_lock(|| {
            let file = self.read_auth_file()?;
            Ok(file
                .servers
                .into_iter()
                .map(|(key, entry)| (ServerTargetKey::from_canonical(key), entry))
                .collect())
        })
    }

    fn read_auth_file(&self) -> Result<AuthFile, AuthStoreError> {
        let contents = fs::read_to_string(&self.path).map_err(|source| AuthStoreError::Read {
            path: self.path.clone(),
            source,
        })?;
        serde_json::from_str(&contents).map_err(|source| AuthStoreError::Corrupt {
            path: self.path.clone(),
            source,
        })
    }

    fn read_auth_file_if_exists(&self) -> Result<AuthFile, AuthStoreError> {
        if !self.path.exists() {
            return Ok(AuthFile::default());
        }
        self.read_auth_file()
    }

    #[cfg(unix)]
    fn ensure_parent_dir(&self) -> Result<(), AuthStoreError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|source| AuthStoreError::CreateDir {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        Ok(())
    }

    #[cfg(unix)]
    fn with_shared_lock<T>(
        &self,
        f: impl FnOnce() -> Result<T, AuthStoreError>,
    ) -> Result<T, AuthStoreError> {
        let lock_file = self.open_lock_file()?;
        match FileExt::try_lock_shared(&lock_file) {
            Ok(()) => {}
            Err(source) if source.kind() == std::io::ErrorKind::WouldBlock => {
                lock_file
                    .lock_shared()
                    .map_err(|source| self.lock_error(source))?;
            }
            Err(source) => return Err(self.lock_error(source)),
        }
        f()
    }

    #[cfg(not(unix))]
    fn with_shared_lock<T>(
        &self,
        f: impl FnOnce() -> Result<T, AuthStoreError>,
    ) -> Result<T, AuthStoreError> {
        f()
    }

    #[cfg(unix)]
    fn with_exclusive_lock<T>(
        &self,
        f: impl FnOnce() -> Result<T, AuthStoreError>,
    ) -> Result<T, AuthStoreError> {
        let lock_file = self.open_lock_file()?;
        match FileExt::try_lock_exclusive(&lock_file) {
            Ok(()) => {}
            Err(source) if source.kind() == std::io::ErrorKind::WouldBlock => {
                lock_file
                    .lock_exclusive()
                    .map_err(|source| self.lock_error(source))?;
            }
            Err(source) => return Err(self.lock_error(source)),
        }
        f()
    }

    #[cfg(unix)]
    fn open_lock_file(&self) -> Result<std::fs::File, AuthStoreError> {
        let path = self.lock_path();
        std::fs::OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(&path)
            .map_err(|source| LockError::Io { path, source }.into())
    }

    fn lock_path(&self) -> PathBuf {
        self.path.with_extension("lock")
    }

    #[cfg(unix)]
    fn lock_error(&self, source: std::io::Error) -> AuthStoreError {
        classify_lock_error(self.lock_path(), source).into()
    }

    #[cfg(unix)]
    fn write_auth_file(&self, file: &AuthFile) -> Result<(), AuthStoreError> {
        let serialized =
            serde_json::to_string_pretty(file).map_err(|source| AuthStoreError::Serialize {
                path: self.path.clone(),
                source,
            })?;

        let temp_path = self.path.with_file_name(format!(
            ".{}.tmp-{:x}",
            self.path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("auth"),
            rand::rng().random::<u64>()
        ));

        write_private_file(&temp_path, &format!("{serialized}\n"))?;
        fs::rename(&temp_path, &self.path).map_err(|source| AuthStoreError::Write {
            path: self.path.clone(),
            source,
        })?;
        Ok(())
    }
}

fn canonical_http_target(api_url: &str) -> Result<String, AuthStoreError> {
    let normalized = api_url
        .trim()
        .trim_end_matches('/')
        .strip_suffix("/api/v1")
        .unwrap_or(api_url.trim().trim_end_matches('/'))
        .to_string();
    let url =
        fabro_http::Url::parse(&normalized).map_err(|_| AuthStoreError::InvalidServerTarget {
            value: api_url.to_string(),
        })?;
    let scheme = url.scheme().to_ascii_lowercase();
    let Some(host) = url.host_str() else {
        return Err(AuthStoreError::InvalidServerTarget {
            value: api_url.to_string(),
        });
    };
    let host = host.to_ascii_lowercase();
    let Some(port) = url.port_or_known_default() else {
        return Err(AuthStoreError::InvalidServerTarget {
            value: api_url.to_string(),
        });
    };
    let default_port = match scheme.as_str() {
        "http" => 80,
        "https" => 443,
        _ => {
            return Err(AuthStoreError::InvalidServerTarget {
                value: api_url.to_string(),
            });
        }
    };
    if port == default_port {
        Ok(format!("{scheme}://{host}"))
    } else {
        Ok(format!("{scheme}://{host}:{port}"))
    }
}

fn canonical_socket_path(path: &Path) -> Result<PathBuf, AuthStoreError> {
    if !path.is_absolute() {
        return Err(AuthStoreError::InvalidServerTarget {
            value: path.display().to_string(),
        });
    }
    Ok(fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf()))
}

#[cfg(unix)]
fn write_private_file(path: &Path, contents: &str) -> Result<(), AuthStoreError> {
    use std::fs::OpenOptions;
    use std::os::unix::fs::OpenOptionsExt;

    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
        .map_err(|source| AuthStoreError::Write {
            path: path.to_path_buf(),
            source,
        })?;
    file.write_all(contents.as_bytes())
        .map_err(|source| AuthStoreError::Write {
            path: path.to_path_buf(),
            source,
        })?;
    file.sync_all().map_err(|source| AuthStoreError::Write {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(())
}

#[cfg(unix)]
fn classify_lock_error(path: PathBuf, source: std::io::Error) -> LockError {
    match source.raw_os_error() {
        Some(code) if code == libc::EOPNOTSUPP || code == libc::ENOLCK => {
            LockError::FilesystemDoesNotSupportLocking { path }
        }
        _ => LockError::Io { path, source },
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::thread;

    use chrono::Duration;

    #[cfg(unix)]
    use super::{AUTH_FILE_ENV, LockError, classify_lock_error};
    use super::{AuthEntry, AuthStore, ServerTargetKey, Subject};
    use crate::user_config::ServerTarget;

    fn entry(login: &str) -> AuthEntry {
        let now = chrono::Utc::now();
        AuthEntry {
            access_token:             format!("access-{login}"),
            access_token_expires_at:  now + Duration::minutes(10),
            refresh_token:            format!("refresh-{login}"),
            refresh_token_expires_at: now + Duration::days(30),
            subject:                  Subject {
                idp_issuer:  "https://github.com".to_string(),
                idp_subject: "12345".to_string(),
                login:       login.to_string(),
                name:        format!("Name {login}"),
                email:       format!("{login}@example.com"),
            },
            logged_in_at:             now,
        }
    }

    fn https_target(value: &str) -> ServerTarget {
        ServerTarget::HttpUrl {
            api_url: value.to_string(),
            tls:     None,
        }
    }

    #[cfg(unix)]
    #[test]
    fn round_trips_https_entry() {
        let temp = tempfile::tempdir().unwrap();
        let store = AuthStore::new(temp.path().join("auth.json"));
        let key = ServerTargetKey::new(&https_target("https://fabro.example.com")).unwrap();

        store.put(&key, entry("octocat")).unwrap();

        let saved = store.get(&key).unwrap().unwrap();
        assert_eq!(saved.subject.login, "octocat");
    }

    #[cfg(unix)]
    #[test]
    fn round_trips_loopback_http_entry() {
        let temp = tempfile::tempdir().unwrap();
        let store = AuthStore::new(temp.path().join("auth.json"));
        let key = ServerTargetKey::new(&https_target("http://127.0.0.1:3000")).unwrap();

        store.put(&key, entry("alice")).unwrap();

        let saved = store.get(&key).unwrap().unwrap();
        assert_eq!(saved.subject.login, "alice");
    }

    #[cfg(unix)]
    #[test]
    fn round_trips_unix_socket_entry() {
        let temp = tempfile::tempdir().unwrap();
        let socket = temp.path().join("fabro.sock");
        std::fs::write(&socket, "").unwrap();
        let store = AuthStore::new(temp.path().join("auth.json"));
        let key = ServerTargetKey::new(&ServerTarget::UnixSocket(socket)).unwrap();

        store.put(&key, entry("unix")).unwrap();

        let saved = store.get(&key).unwrap().unwrap();
        assert_eq!(saved.subject.login, "unix");
    }

    #[test]
    fn https_normalization_collapses_equivalent_urls() {
        let a = ServerTargetKey::new(&https_target("https://EXAMPLE.COM/")).unwrap();
        let b = ServerTargetKey::new(&https_target("https://example.com:443")).unwrap();
        let c = ServerTargetKey::new(&https_target("https://example.com")).unwrap();

        assert_eq!(a, b);
        assert_eq!(b, c);
    }

    #[test]
    fn distinct_unix_socket_paths_do_not_collide() {
        let a =
            ServerTargetKey::new(&ServerTarget::UnixSocket(PathBuf::from("/tmp/a.sock"))).unwrap();
        let b =
            ServerTargetKey::new(&ServerTarget::UnixSocket(PathBuf::from("/tmp/b.sock"))).unwrap();

        assert_ne!(a, b);
    }

    #[cfg(unix)]
    #[test]
    fn canonicalizes_symlinked_socket_paths() {
        let temp = tempfile::tempdir().unwrap();
        let socket = temp.path().join("fabro.sock");
        let link = temp.path().join("fabro-link.sock");
        std::fs::write(&socket, "").unwrap();
        std::os::unix::fs::symlink(&socket, &link).unwrap();

        let direct = ServerTargetKey::new(&ServerTarget::UnixSocket(socket)).unwrap();
        let via_link = ServerTargetKey::new(&ServerTarget::UnixSocket(link)).unwrap();

        assert_eq!(direct, via_link);
    }

    #[test]
    fn missing_file_returns_empty_results() {
        let temp = tempfile::tempdir().unwrap();
        let store = AuthStore::new(temp.path().join("auth.json"));
        let key = ServerTargetKey::new(&https_target("https://fabro.example.com")).unwrap();

        assert!(store.get(&key).unwrap().is_none());
        assert!(store.list().unwrap().is_empty());
    }

    #[test]
    fn corrupt_file_returns_clear_error() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("auth.json");
        std::fs::write(&path, "{not-json").unwrap();
        let store = AuthStore::new(path.clone());
        let key = ServerTargetKey::new(&https_target("https://fabro.example.com")).unwrap();

        let err = store.get(&key).unwrap_err();
        assert!(err.to_string().contains(&path.display().to_string()));
    }

    #[cfg(unix)]
    #[test]
    fn concurrent_puts_do_not_corrupt_file() {
        let temp = tempfile::tempdir().unwrap();
        let store = Arc::new(AuthStore::new(temp.path().join("auth.json")));
        let key = ServerTargetKey::new(&https_target("https://fabro.example.com")).unwrap();

        let mut tasks = Vec::new();
        for login in ["alice", "bob"] {
            let store = Arc::clone(&store);
            let key = key.clone();
            tasks.push(thread::spawn(move || {
                store.put(&key, entry(login)).unwrap();
            }));
        }
        for task in tasks {
            task.join().unwrap();
        }

        let saved = store.get(&key).unwrap().unwrap();
        assert!(matches!(saved.subject.login.as_str(), "alice" | "bob"));
    }

    #[cfg(unix)]
    #[test]
    fn writes_files_with_private_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().unwrap();
        let store = AuthStore::new(temp.path().join("auth.json"));
        let key = ServerTargetKey::new(&https_target("https://fabro.example.com")).unwrap();

        store.put(&key, entry("octocat")).unwrap();

        let mode = std::fs::metadata(temp.path().join("auth.json"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[cfg(not(unix))]
    #[test]
    fn put_returns_unsupported_platform() {
        let temp = tempfile::tempdir().unwrap();
        let store = AuthStore::new(temp.path().join("auth.json"));
        let key = ServerTargetKey::new(&https_target("https://fabro.example.com")).unwrap();

        let err = store.put(&key, entry("octocat")).unwrap_err();
        assert!(err.to_string().contains("not supported on this platform"));
    }

    #[cfg(unix)]
    #[test]
    fn unsupported_locking_filesystem_returns_actionable_error() {
        let path = PathBuf::from("/tmp/fabro-auth.lock");
        let err = classify_lock_error(
            path.clone(),
            std::io::Error::from_raw_os_error(libc::ENOLCK),
        );

        assert!(matches!(
            err,
            LockError::FilesystemDoesNotSupportLocking { path: ref error_path } if error_path == &path
        ));
        assert!(err.to_string().contains(AUTH_FILE_ENV));
    }
}
