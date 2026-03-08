use base64::Engine;
use base64::engine::general_purpose::STANDARD;

use super::event::Track;

const SEGMENT_API_URL: &str = "https://api.segment.io/v1/track";
const SEGMENT_WRITE_KEY: Option<&str> = option_env!("SEGMENT_WRITE_KEY");

/// Spawns a fire-and-forget tokio task to send a track event to Segment.
/// No-ops if the SEGMENT_WRITE_KEY was not set at compile time.
pub fn send(track: Track) {
    let Some(write_key) = SEGMENT_WRITE_KEY else {
        tracing::debug!("telemetry: no SEGMENT_WRITE_KEY, skipping send");
        return;
    };

    tokio::spawn(send_track(track, write_key));
}

async fn send_track(track: Track, write_key: &str) {
    let auth = STANDARD.encode(format!("{write_key}:"));

    let result = reqwest::Client::new()
        .post(SEGMENT_API_URL)
        .header("Authorization", format!("Basic {auth}"))
        .json(&track)
        .send()
        .await;

    match result {
        Ok(resp) if !resp.status().is_success() => {
            tracing::warn!(
                status = %resp.status(),
                "telemetry: segment API returned non-success status"
            );
        }
        Err(err) => {
            tracing::warn!(%err, "telemetry: failed to send event to segment");
        }
        Ok(_) => {
            tracing::debug!("telemetry: event sent successfully");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::telemetry::event::User;
    use serde_json::json;

    #[test]
    fn send_noops_without_write_key() {
        // SEGMENT_WRITE_KEY is not set at compile time in tests,
        // so send() should return immediately without spawning.
        let track = Track {
            user: User::AnonymousId {
                anonymous_id: "test".to_string(),
            },
            event: "test".to_string(),
            properties: json!({}),
            context: None,
            timestamp: None,
            message_id: "msg-test".to_string(),
        };

        // This should not panic or require a tokio runtime
        // because it returns before reaching tokio::spawn
        send(track);
    }
}
