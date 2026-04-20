use std::sync::Arc;

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use fabro_types::IdpIdentity;
use serde::{Deserialize, Serialize};
use slatedb::WriteBatch;
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::{Result, keys};

const REPLAY_REVOCATION_TTL_SECONDS: i64 = 60;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RefreshToken {
    pub token_hash:   [u8; 32],
    pub chain_id:     Uuid,
    pub identity:     IdpIdentity,
    pub login:        String,
    pub name:         String,
    pub email:        String,
    pub issued_at:    DateTime<Utc>,
    pub expires_at:   DateTime<Utc>,
    pub last_used_at: DateTime<Utc>,
    pub used:         bool,
    pub user_agent:   String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConsumeOutcome {
    Rotated(RefreshToken, RefreshToken),
    Reused(RefreshToken),
    Expired,
    NotFound,
}

pub struct SlateAuthTokenStore {
    db:                 Arc<slatedb::Db>,
    refresh_locks:      DashMap<[u8; 32], Arc<Mutex<()>>>,
    replay_revocations: DashMap<[u8; 32], DateTime<Utc>>,
}

impl std::fmt::Debug for SlateAuthTokenStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SlateAuthTokenStore")
            .finish_non_exhaustive()
    }
}

impl SlateAuthTokenStore {
    pub(crate) fn new(db: Arc<slatedb::Db>) -> Self {
        Self {
            db,
            refresh_locks: DashMap::new(),
            replay_revocations: DashMap::new(),
        }
    }

    pub async fn insert_refresh_token(&self, token: RefreshToken) -> Result<()> {
        self.db
            .put(
                keys::auth_refresh_key(&token.token_hash),
                serde_json::to_vec(&token)?,
            )
            .await?;
        Ok(())
    }

    pub async fn find_refresh_token(&self, token_hash: &[u8; 32]) -> Result<Option<RefreshToken>> {
        self.db
            .get(keys::auth_refresh_key(token_hash))
            .await?
            .map(|bytes| serde_json::from_slice::<RefreshToken>(&bytes))
            .transpose()
            .map_err(Into::into)
    }

    pub async fn consume_and_rotate(
        &self,
        presented_hash: [u8; 32],
        new_token: RefreshToken,
        now: DateTime<Utc>,
    ) -> Result<ConsumeOutcome> {
        let mutex = self
            .refresh_locks
            .entry(presented_hash)
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone();
        let _guard = mutex.lock().await;

        let outcome = match self.find_refresh_token(&presented_hash).await? {
            None => ConsumeOutcome::NotFound,
            Some(existing) if now >= existing.expires_at => ConsumeOutcome::Expired,
            Some(existing) if existing.used => ConsumeOutcome::Reused(existing),
            Some(existing) => {
                let mut old_token = existing.clone();
                old_token.used = true;
                old_token.last_used_at = now;

                let mut batch = WriteBatch::new();
                batch.put(
                    keys::auth_refresh_key(&presented_hash),
                    serde_json::to_vec(&old_token)?,
                );
                batch.put(
                    keys::auth_refresh_key(&new_token.token_hash),
                    serde_json::to_vec(&new_token)?,
                );
                self.db.write(batch).await?;

                ConsumeOutcome::Rotated(old_token, new_token)
            }
        };

        if Arc::strong_count(&mutex) == 2 {
            self.refresh_locks.remove(&presented_hash);
        }

        Ok(outcome)
    }

    pub async fn delete_chain(&self, chain_id: Uuid) -> Result<u64> {
        let mut iter = self.db.scan_prefix(keys::auth_refresh_prefix()).await?;
        let mut keys_to_delete = Vec::new();
        while let Some(entry) = iter.next().await? {
            let token: RefreshToken = serde_json::from_slice(&entry.value)?;
            if token.chain_id == chain_id {
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

    pub async fn gc_expired(&self, cutoff: DateTime<Utc>) -> Result<u64> {
        let mut iter = self.db.scan_prefix(keys::auth_refresh_prefix()).await?;
        let mut keys_to_delete = Vec::new();
        while let Some(entry) = iter.next().await? {
            let token: RefreshToken = serde_json::from_slice(&entry.value)?;
            if token.expires_at <= cutoff {
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

    pub fn mark_refresh_token_replay(&self, token_hash: [u8; 32], now: DateTime<Utc>) {
        self.replay_revocations.insert(
            token_hash,
            now + chrono::Duration::seconds(REPLAY_REVOCATION_TTL_SECONDS),
        );
        self.replay_revocations
            .retain(|_, expires_at| *expires_at > now);
    }

    pub fn was_recently_replay_revoked(&self, token_hash: &[u8; 32], now: DateTime<Utc>) -> bool {
        self.replay_revocations
            .retain(|_, expires_at| *expires_at > now);
        self.replay_revocations
            .get(token_hash)
            .is_some_and(|expires_at| *expires_at > now)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use chrono::Duration as ChronoDuration;
    use object_store::memory::InMemory;
    use tokio::task::JoinSet;
    use uuid::Uuid;

    use super::{ConsumeOutcome, RefreshToken, SlateAuthTokenStore};
    use crate::Database;

    async fn store() -> Arc<SlateAuthTokenStore> {
        let db = Database::new(
            Arc::new(InMemory::new()),
            "",
            Duration::from_millis(1),
            None,
        );
        db.auth_tokens().await.unwrap()
    }

    fn refresh_token(hash: [u8; 32], chain_id: Uuid, used: bool) -> RefreshToken {
        let now = chrono::Utc::now();
        RefreshToken {
            token_hash: hash,
            chain_id,
            identity: fabro_types::IdpIdentity::new("https://github.com", "12345").unwrap(),
            login: "octocat".to_string(),
            name: "The Octocat".to_string(),
            email: "octocat@example.com".to_string(),
            issued_at: now,
            expires_at: now + ChronoDuration::days(30),
            last_used_at: now,
            used,
            user_agent: "fabro-test".to_string(),
        }
    }

    #[tokio::test]
    async fn insert_find_rotate_and_reuse_work() {
        let store = store().await;
        let chain_id = Uuid::new_v4();
        let old_hash = [1_u8; 32];
        let new_hash = [2_u8; 32];
        let old = refresh_token(old_hash, chain_id, false);
        let new = refresh_token(new_hash, chain_id, false);
        store.insert_refresh_token(old.clone()).await.unwrap();

        assert_eq!(
            store.find_refresh_token(&old_hash).await.unwrap(),
            Some(old)
        );

        let rotated = store
            .consume_and_rotate(old_hash, new.clone(), chrono::Utc::now())
            .await
            .unwrap();
        let ConsumeOutcome::Rotated(old_used, new_saved) = rotated else {
            panic!("expected rotation");
        };
        assert!(old_used.used);
        assert_eq!(new_saved.token_hash, new_hash);
        assert_eq!(
            store.find_refresh_token(&old_hash).await.unwrap(),
            Some(old_used.clone())
        );
        assert_eq!(
            store
                .find_refresh_token(&old_hash)
                .await
                .unwrap()
                .expect("rotated old token should still exist")
                .used,
            true
        );

        let replay = store
            .consume_and_rotate(
                old_hash,
                refresh_token([3_u8; 32], chain_id, false),
                chrono::Utc::now(),
            )
            .await
            .unwrap();
        let ConsumeOutcome::Reused(reused) = replay else {
            panic!("expected replay to return the original used row");
        };
        assert_eq!(reused.chain_id, chain_id);
    }

    #[tokio::test]
    async fn missing_and_expired_tokens_are_reported() {
        let store = store().await;
        let chain_id = Uuid::new_v4();

        assert_eq!(
            store
                .consume_and_rotate(
                    [7_u8; 32],
                    refresh_token([8_u8; 32], chain_id, false),
                    chrono::Utc::now(),
                )
                .await
                .unwrap(),
            ConsumeOutcome::NotFound
        );

        let mut expired = refresh_token([9_u8; 32], chain_id, false);
        expired.expires_at = chrono::Utc::now() - ChronoDuration::seconds(1);
        store.insert_refresh_token(expired.clone()).await.unwrap();

        assert_eq!(
            store
                .consume_and_rotate(
                    expired.token_hash,
                    refresh_token([10_u8; 32], chain_id, false),
                    chrono::Utc::now(),
                )
                .await
                .unwrap(),
            ConsumeOutcome::Expired
        );
        assert_eq!(
            store.find_refresh_token(&expired.token_hash).await.unwrap(),
            Some(expired)
        );
    }

    #[tokio::test]
    async fn concurrent_rotation_has_one_winner() {
        let store = store().await;
        let chain_id = Uuid::new_v4();
        let hash = [9_u8; 32];
        store
            .insert_refresh_token(refresh_token(hash, chain_id, false))
            .await
            .unwrap();

        let mut tasks = JoinSet::new();
        for idx in 0..16_u8 {
            let store = Arc::clone(&store);
            tasks.spawn(async move {
                store
                    .consume_and_rotate(
                        hash,
                        refresh_token([idx; 32], chain_id, false),
                        chrono::Utc::now(),
                    )
                    .await
                    .unwrap()
            });
        }

        let mut rotated = 0;
        let mut reused = 0;
        while let Some(result) = tasks.join_next().await {
            match result.unwrap() {
                ConsumeOutcome::Rotated(_, _) => rotated += 1,
                ConsumeOutcome::Reused(_) => reused += 1,
                other => panic!("unexpected outcome: {other:?}"),
            }
        }

        assert_eq!(rotated, 1);
        assert_eq!(reused, 15);
    }

    #[tokio::test]
    async fn delete_chain_removes_all_matching_tokens() {
        let store = store().await;
        let chain_id = Uuid::new_v4();
        store
            .insert_refresh_token(refresh_token([1_u8; 32], chain_id, false))
            .await
            .unwrap();
        store
            .insert_refresh_token(refresh_token([2_u8; 32], chain_id, true))
            .await
            .unwrap();

        assert_eq!(store.delete_chain(chain_id).await.unwrap(), 2);
        assert!(
            store
                .find_refresh_token(&[1_u8; 32])
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            store
                .find_refresh_token(&[2_u8; 32])
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn gc_expired_removes_only_tokens_at_or_before_cutoff() {
        let store = store().await;
        let chain_id = Uuid::new_v4();

        let mut expired = refresh_token([4_u8; 32], chain_id, true);
        expired.expires_at = chrono::Utc::now() - ChronoDuration::days(8);
        let live = refresh_token([5_u8; 32], chain_id, false);

        store.insert_refresh_token(expired.clone()).await.unwrap();
        store.insert_refresh_token(live.clone()).await.unwrap();

        assert_eq!(store.gc_expired(chrono::Utc::now()).await.unwrap(), 1);
        assert!(
            store
                .find_refresh_token(&expired.token_hash)
                .await
                .unwrap()
                .is_none()
        );
        assert_eq!(
            store.find_refresh_token(&live.token_hash).await.unwrap(),
            Some(live)
        );
    }
}
