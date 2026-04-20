use std::sync::Arc;

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use fabro_types::IdpIdentity;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::{Result, keys};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthCode {
    pub identity:       IdpIdentity,
    pub login:          String,
    pub name:           String,
    pub email:          String,
    pub code_challenge: String,
    pub redirect_uri:   String,
    pub expires_at:     DateTime<Utc>,
}

pub struct SlateAuthCodeStore {
    db:         Arc<slatedb::Db>,
    code_locks: DashMap<String, Arc<Mutex<()>>>,
}

impl std::fmt::Debug for SlateAuthCodeStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SlateAuthCodeStore").finish_non_exhaustive()
    }
}

impl SlateAuthCodeStore {
    pub(crate) fn new(db: Arc<slatedb::Db>) -> Self {
        Self {
            db,
            code_locks: DashMap::new(),
        }
    }

    pub async fn insert(&self, code: &str, entry: AuthCode) -> Result<()> {
        self.db
            .put(keys::auth_code_key(code), serde_json::to_vec(&entry)?)
            .await?;
        Ok(())
    }

    pub async fn consume(&self, code: &str) -> Result<Option<AuthCode>> {
        let mutex = self
            .code_locks
            .entry(code.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone();
        let _guard = mutex.lock().await;

        let key = keys::auth_code_key(code);
        let entry = self
            .db
            .get(&key)
            .await?
            .map(|bytes| serde_json::from_slice::<AuthCode>(&bytes))
            .transpose()?;
        let result = match entry {
            Some(entry) if entry.expires_at > Utc::now() => {
                self.db.delete(&key).await?;
                Some(entry)
            }
            Some(_) => {
                self.db.delete(&key).await?;
                None
            }
            None => None,
        };

        if Arc::strong_count(&mutex) == 2 {
            self.code_locks.remove(code);
        }

        Ok(result)
    }

    pub async fn gc_expired(&self, cutoff: DateTime<Utc>) -> Result<u64> {
        let mut iter = self.db.scan_prefix(keys::auth_code_prefix()).await?;
        let mut keys_to_delete = Vec::new();
        while let Some(entry) = iter.next().await? {
            let auth_code: AuthCode = serde_json::from_slice(&entry.value)?;
            if auth_code.expires_at <= cutoff {
                keys_to_delete.push(
                    String::from_utf8(entry.key.to_vec())
                        .expect("slatedb keys should be valid utf-8"),
                );
            }
        }

        for key in &keys_to_delete {
            self.db.delete(key).await?;
        }

        Ok(keys_to_delete.len() as u64)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use chrono::Duration as ChronoDuration;
    use object_store::memory::InMemory;
    use tokio::task::JoinSet;

    use super::{AuthCode, SlateAuthCodeStore};
    use crate::Database;

    async fn store() -> Arc<SlateAuthCodeStore> {
        let db = Database::new(
            Arc::new(InMemory::new()),
            "",
            Duration::from_millis(1),
            None,
        );
        db.auth_codes().await.unwrap()
    }

    fn auth_code(expires_at: chrono::DateTime<chrono::Utc>) -> AuthCode {
        AuthCode {
            identity: fabro_types::IdpIdentity::new("https://github.com", "12345").unwrap(),
            login: "octocat".to_string(),
            name: "The Octocat".to_string(),
            email: "octocat@example.com".to_string(),
            code_challenge: "challenge".to_string(),
            redirect_uri: "http://127.0.0.1/callback".to_string(),
            expires_at,
        }
    }

    #[tokio::test]
    async fn insert_and_consume_is_single_use() {
        let store = store().await;
        store
            .insert(
                "code-1",
                auth_code(chrono::Utc::now() + ChronoDuration::seconds(60)),
            )
            .await
            .unwrap();

        assert!(store.consume("code-1").await.unwrap().is_some());
        assert!(store.consume("code-1").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn concurrent_consume_has_one_winner() {
        let store = store().await;
        store
            .insert(
                "code-2",
                auth_code(chrono::Utc::now() + ChronoDuration::seconds(60)),
            )
            .await
            .unwrap();

        let mut tasks = JoinSet::new();
        for _ in 0..16 {
            let store = Arc::clone(&store);
            tasks.spawn(async move { store.consume("code-2").await.unwrap().is_some() });
        }

        let mut successes = 0;
        while let Some(result) = tasks.join_next().await {
            if result.unwrap() {
                successes += 1;
            }
        }

        assert_eq!(successes, 1);
    }

    #[tokio::test]
    async fn gc_expired_removes_only_expired_codes() {
        let store = store().await;
        store
            .insert(
                "expired",
                auth_code(chrono::Utc::now() - ChronoDuration::seconds(1)),
            )
            .await
            .unwrap();
        store
            .insert(
                "live",
                auth_code(chrono::Utc::now() + ChronoDuration::seconds(60)),
            )
            .await
            .unwrap();

        assert_eq!(store.gc_expired(chrono::Utc::now()).await.unwrap(), 1);
        assert!(store.consume("expired").await.unwrap().is_none());
        assert!(store.consume("live").await.unwrap().is_some());
    }
}
