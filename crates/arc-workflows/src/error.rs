use std::fmt;
use std::str::FromStr;

use arc_llm::error::{ProviderErrorKind, SdkError};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Classification of failure modes for pipeline edge conditions.
///
/// Pipeline authors can write edge conditions like `context.failure_class=budget_exhausted`
/// to route execution based on the nature of the failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureClass {
    /// Temporary infrastructure failure (rate limit, timeout, network, 5xx).
    TransientInfra,
    /// Permanent failure (auth, bad config, code bug).
    Deterministic,
    /// Context length, token/turn limit, quota exceeded.
    BudgetExhausted,
    /// Reserved for future loop detection.
    CompilationLoop,
    /// User/system cancellation.
    Canceled,
    /// Reserved for future scope enforcement.
    Structural,
}

impl fmt::Display for FailureClass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::TransientInfra => "transient_infra",
            Self::Deterministic => "deterministic",
            Self::BudgetExhausted => "budget_exhausted",
            Self::CompilationLoop => "compilation_loop",
            Self::Canceled => "canceled",
            Self::Structural => "structural",
        };
        write!(f, "{s}")
    }
}

impl FromStr for FailureClass {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "transient_infra" => Ok(Self::TransientInfra),
            "deterministic" => Ok(Self::Deterministic),
            "budget_exhausted" => Ok(Self::BudgetExhausted),
            "compilation_loop" => Ok(Self::CompilationLoop),
            "canceled" => Ok(Self::Canceled),
            "structural" => Ok(Self::Structural),
            other => Err(format!("unknown failure class: {other}")),
        }
    }
}

/// Classify an `SdkError` into a `FailureClass` based on its structure.
#[must_use]
pub fn classify_sdk_error(err: &SdkError) -> FailureClass {
    match err {
        SdkError::Provider { kind, .. } => match kind {
            ProviderErrorKind::RateLimit | ProviderErrorKind::Server => {
                FailureClass::TransientInfra
            }
            ProviderErrorKind::ContextLength | ProviderErrorKind::QuotaExceeded => {
                FailureClass::BudgetExhausted
            }
            ProviderErrorKind::Authentication
            | ProviderErrorKind::AccessDenied
            | ProviderErrorKind::NotFound
            | ProviderErrorKind::InvalidRequest
            | ProviderErrorKind::ContentFilter => FailureClass::Deterministic,
        },
        SdkError::RequestTimeout { .. } | SdkError::Network { .. } | SdkError::Stream { .. } => {
            FailureClass::TransientInfra
        }
        SdkError::Abort { .. } => FailureClass::Canceled,
        SdkError::InvalidToolCall { .. }
        | SdkError::NoObjectGenerated { .. }
        | SdkError::Configuration { .. }
        | SdkError::UnsupportedToolChoice { .. } => FailureClass::Deterministic,
    }
}

/// Classify a failure reason string using heuristics.
///
/// This is the fallback when structured error information is not available
/// (e.g. for `Handler(String)` or `Engine(String)` errors).
#[must_use]
pub fn classify_failure_reason(reason: &str) -> FailureClass {
    let lower = reason.to_lowercase();

    if lower.contains("cancel") || lower.contains("abort") {
        return FailureClass::Canceled;
    }

    if lower.contains("turn limit")
        || lower.contains("token limit")
        || lower.contains("context length")
        || lower.contains("budget")
        || lower.contains("quota exceeded")
    {
        return FailureClass::BudgetExhausted;
    }

    if lower.contains("scope violation") {
        return FailureClass::Structural;
    }

    if lower.contains("timeout")
        || lower.contains("timed out")
        || lower.contains("rate limit")
        || lower.contains("rate limited")
        || lower.contains("connection refused")
        || lower.contains("connection reset")
        || lower.contains("503")
        || lower.contains("502")
        || lower.contains("500")
    {
        return FailureClass::TransientInfra;
    }

    FailureClass::Deterministic
}

#[derive(Error, Debug, Clone)]
pub enum ArcError {
    #[error("Parse error: {0}")]
    Parse(String),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Engine error: {0}")]
    Engine(String),

    #[error("Handler error: {0}")]
    Handler(String),

    #[error("LLM error: {0}")]
    Llm(SdkError),

    #[error("Checkpoint error: {0}")]
    Checkpoint(String),

    #[error("Stylesheet error: {0}")]
    Stylesheet(String),

    #[error("I/O error: {0}")]
    Io(String),

    #[error("Pipeline cancelled")]
    Cancelled,
}

impl ArcError {
    /// Whether this error category is retryable (transient) or terminal.
    ///
    /// Retryable: Handler (transient handler failures), Engine (could be transient),
    ///            Io (network/disk issues are often transient), Llm (delegates to SdkError).
    /// Terminal:  Parse, Validation, Stylesheet (configuration errors),
    ///            Checkpoint (storage integrity), Cancelled (explicit cancellation).
    #[must_use]
    pub fn is_retryable(&self) -> bool {
        match self {
            Self::Handler(_) | Self::Engine(_) | Self::Io(_) => true,
            Self::Llm(sdk_err) => sdk_err.retryable(),
            Self::Parse(_)
            | Self::Validation(_)
            | Self::Stylesheet(_)
            | Self::Checkpoint(_)
            | Self::Cancelled => false,
        }
    }

    /// Classify this error into a `FailureClass`.
    #[must_use]
    pub fn failure_class(&self) -> FailureClass {
        match self {
            Self::Cancelled => FailureClass::Canceled,
            Self::Llm(sdk_err) => classify_sdk_error(sdk_err),
            Self::Io(_) => FailureClass::TransientInfra,
            Self::Parse(_) | Self::Validation(_) | Self::Stylesheet(_) | Self::Checkpoint(_) => {
                FailureClass::Deterministic
            }
            Self::Handler(msg) | Self::Engine(msg) => classify_failure_reason(msg),
        }
    }
}

impl From<std::io::Error> for ArcError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err.to_string())
    }
}

impl From<SdkError> for ArcError {
    fn from(err: SdkError) -> Self {
        Self::Llm(err)
    }
}

pub type Result<T> = std::result::Result<T, ArcError>;

#[cfg(test)]
mod tests {
    use super::*;
    use arc_llm::error::ProviderErrorDetail;

    #[test]
    fn parse_error_display() {
        let err = ArcError::Parse("unexpected token".to_string());
        assert_eq!(err.to_string(), "Parse error: unexpected token");
    }

    #[test]
    fn validation_error_display() {
        let err = ArcError::Validation("missing start node".to_string());
        assert_eq!(err.to_string(), "Validation error: missing start node");
    }

    #[test]
    fn engine_error_display() {
        let err = ArcError::Engine("no outgoing edge".to_string());
        assert_eq!(err.to_string(), "Engine error: no outgoing edge");
    }

    #[test]
    fn handler_error_display() {
        let err = ArcError::Handler("LLM call failed".to_string());
        assert_eq!(err.to_string(), "Handler error: LLM call failed");
    }

    #[test]
    fn checkpoint_error_display() {
        let err = ArcError::Checkpoint("file not found".to_string());
        assert_eq!(err.to_string(), "Checkpoint error: file not found");
    }

    #[test]
    fn io_error_display() {
        let err = ArcError::Io("permission denied".to_string());
        assert_eq!(err.to_string(), "I/O error: permission denied");
    }

    #[test]
    fn io_error_from_std() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "not found");
        let err = ArcError::from(io_err);
        assert!(matches!(err, ArcError::Io(_)));
        assert!(err.to_string().contains("not found"));
    }

    #[test]
    fn result_type_alias_works() {
        let ok: Result<i32> = Ok(42);
        assert!(ok.is_ok());

        let err: Result<i32> = Err(ArcError::Parse("bad".to_string()));
        assert!(err.is_err());
    }

    #[test]
    fn cancelled_error_display() {
        let err = ArcError::Cancelled;
        assert_eq!(err.to_string(), "Pipeline cancelled");
    }

    #[test]
    fn cancelled_is_not_retryable() {
        assert!(!ArcError::Cancelled.is_retryable());
    }

    #[test]
    fn is_retryable_terminal_errors() {
        assert!(!ArcError::Parse("bad".to_string()).is_retryable());
        assert!(!ArcError::Validation("bad".to_string()).is_retryable());
        assert!(!ArcError::Stylesheet("bad".to_string()).is_retryable());
        assert!(!ArcError::Checkpoint("bad".to_string()).is_retryable());
    }

    #[test]
    fn is_retryable_transient_errors() {
        assert!(ArcError::Handler("timeout".to_string()).is_retryable());
        assert!(ArcError::Engine("transient".to_string()).is_retryable());
        assert!(ArcError::Io("connection reset".to_string()).is_retryable());
    }

    // --- FailureClass Display/FromStr/serde tests ---

    #[test]
    fn failure_class_display_all_values() {
        assert_eq!(FailureClass::TransientInfra.to_string(), "transient_infra");
        assert_eq!(FailureClass::Deterministic.to_string(), "deterministic");
        assert_eq!(FailureClass::BudgetExhausted.to_string(), "budget_exhausted");
        assert_eq!(FailureClass::CompilationLoop.to_string(), "compilation_loop");
        assert_eq!(FailureClass::Canceled.to_string(), "canceled");
        assert_eq!(FailureClass::Structural.to_string(), "structural");
    }

    #[test]
    fn failure_class_from_str_all_values() {
        assert_eq!(
            "transient_infra".parse::<FailureClass>().unwrap(),
            FailureClass::TransientInfra
        );
        assert_eq!(
            "deterministic".parse::<FailureClass>().unwrap(),
            FailureClass::Deterministic
        );
        assert_eq!(
            "budget_exhausted".parse::<FailureClass>().unwrap(),
            FailureClass::BudgetExhausted
        );
        assert_eq!(
            "compilation_loop".parse::<FailureClass>().unwrap(),
            FailureClass::CompilationLoop
        );
        assert_eq!(
            "canceled".parse::<FailureClass>().unwrap(),
            FailureClass::Canceled
        );
        assert_eq!(
            "structural".parse::<FailureClass>().unwrap(),
            FailureClass::Structural
        );
    }

    #[test]
    fn failure_class_from_str_invalid() {
        assert!("unknown".parse::<FailureClass>().is_err());
    }

    #[test]
    fn failure_class_serde_roundtrip() {
        let values = [
            FailureClass::TransientInfra,
            FailureClass::Deterministic,
            FailureClass::BudgetExhausted,
            FailureClass::CompilationLoop,
            FailureClass::Canceled,
            FailureClass::Structural,
        ];
        for fc in values {
            let json = serde_json::to_string(&fc).unwrap();
            let parsed: FailureClass = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed, fc);
        }
    }

    // --- Llm variant tests ---

    #[test]
    fn llm_error_display() {
        let sdk_err = SdkError::Network {
            message: "connection refused".into(),
        };
        let err = ArcError::Llm(sdk_err);
        assert_eq!(err.to_string(), "LLM error: Network error: connection refused");
    }

    #[test]
    fn llm_error_retryable_delegates_to_sdk() {
        let retryable = ArcError::Llm(SdkError::Network {
            message: "timeout".into(),
        });
        assert!(retryable.is_retryable());

        let non_retryable = ArcError::Llm(SdkError::Configuration {
            message: "bad config".into(),
        });
        assert!(!non_retryable.is_retryable());
    }

    #[test]
    fn llm_error_from_sdk_error() {
        let sdk_err = SdkError::Stream {
            message: "broken pipe".into(),
        };
        let err = ArcError::from(sdk_err);
        assert!(matches!(err, ArcError::Llm(_)));
    }

    // --- failure_class() method tests ---

    #[test]
    fn failure_class_cancelled() {
        assert_eq!(
            ArcError::Cancelled.failure_class(),
            FailureClass::Canceled
        );
    }

    #[test]
    fn failure_class_io() {
        assert_eq!(
            ArcError::Io("disk full".into()).failure_class(),
            FailureClass::TransientInfra
        );
    }

    #[test]
    fn failure_class_parse() {
        assert_eq!(
            ArcError::Parse("bad syntax".into()).failure_class(),
            FailureClass::Deterministic
        );
    }

    #[test]
    fn failure_class_handler_with_timeout() {
        assert_eq!(
            ArcError::Handler("request timed out".into()).failure_class(),
            FailureClass::TransientInfra
        );
    }

    #[test]
    fn failure_class_handler_deterministic() {
        assert_eq!(
            ArcError::Handler("invalid configuration".into()).failure_class(),
            FailureClass::Deterministic
        );
    }

    #[test]
    fn failure_class_llm_rate_limit() {
        let err = ArcError::Llm(SdkError::Provider {
            kind: ProviderErrorKind::RateLimit,
            detail: Box::new(ProviderErrorDetail::new("too fast", "openai")),
        });
        assert_eq!(err.failure_class(), FailureClass::TransientInfra);
    }

    #[test]
    fn failure_class_llm_context_length() {
        let err = ArcError::Llm(SdkError::Provider {
            kind: ProviderErrorKind::ContextLength,
            detail: Box::new(ProviderErrorDetail::new("too long", "openai")),
        });
        assert_eq!(err.failure_class(), FailureClass::BudgetExhausted);
    }

    #[test]
    fn failure_class_llm_auth() {
        let err = ArcError::Llm(SdkError::Provider {
            kind: ProviderErrorKind::Authentication,
            detail: Box::new(ProviderErrorDetail::new("bad key", "openai")),
        });
        assert_eq!(err.failure_class(), FailureClass::Deterministic);
    }

    #[test]
    fn failure_class_llm_abort() {
        let err = ArcError::Llm(SdkError::Abort {
            message: "user cancelled".into(),
        });
        assert_eq!(err.failure_class(), FailureClass::Canceled);
    }

    #[test]
    fn failure_class_llm_timeout() {
        let err = ArcError::Llm(SdkError::RequestTimeout {
            message: "timed out".into(),
        });
        assert_eq!(err.failure_class(), FailureClass::TransientInfra);
    }

    // --- classify_sdk_error tests ---

    #[test]
    fn classify_sdk_rate_limit() {
        let err = SdkError::Provider {
            kind: ProviderErrorKind::RateLimit,
            detail: Box::new(ProviderErrorDetail::new("too fast", "openai")),
        };
        assert_eq!(classify_sdk_error(&err), FailureClass::TransientInfra);
    }

    #[test]
    fn classify_sdk_server() {
        let err = SdkError::Provider {
            kind: ProviderErrorKind::Server,
            detail: Box::new(ProviderErrorDetail::new("500", "openai")),
        };
        assert_eq!(classify_sdk_error(&err), FailureClass::TransientInfra);
    }

    #[test]
    fn classify_sdk_context_length() {
        let err = SdkError::Provider {
            kind: ProviderErrorKind::ContextLength,
            detail: Box::new(ProviderErrorDetail::new("too long", "openai")),
        };
        assert_eq!(classify_sdk_error(&err), FailureClass::BudgetExhausted);
    }

    #[test]
    fn classify_sdk_quota_exceeded() {
        let err = SdkError::Provider {
            kind: ProviderErrorKind::QuotaExceeded,
            detail: Box::new(ProviderErrorDetail::new("out of quota", "openai")),
        };
        assert_eq!(classify_sdk_error(&err), FailureClass::BudgetExhausted);
    }

    #[test]
    fn classify_sdk_auth() {
        let err = SdkError::Provider {
            kind: ProviderErrorKind::Authentication,
            detail: Box::new(ProviderErrorDetail::new("bad key", "openai")),
        };
        assert_eq!(classify_sdk_error(&err), FailureClass::Deterministic);
    }

    #[test]
    fn classify_sdk_request_timeout() {
        let err = SdkError::RequestTimeout {
            message: "timed out".into(),
        };
        assert_eq!(classify_sdk_error(&err), FailureClass::TransientInfra);
    }

    #[test]
    fn classify_sdk_abort() {
        let err = SdkError::Abort {
            message: "cancelled".into(),
        };
        assert_eq!(classify_sdk_error(&err), FailureClass::Canceled);
    }

    #[test]
    fn classify_sdk_invalid_tool_call() {
        let err = SdkError::InvalidToolCall {
            message: "bad tool".into(),
        };
        assert_eq!(classify_sdk_error(&err), FailureClass::Deterministic);
    }

    // --- classify_failure_reason tests ---

    #[test]
    fn classify_reason_timeout() {
        assert_eq!(
            classify_failure_reason("request timed out after 30s"),
            FailureClass::TransientInfra
        );
    }

    #[test]
    fn classify_reason_rate_limit() {
        assert_eq!(
            classify_failure_reason("rate limited by provider"),
            FailureClass::TransientInfra
        );
    }

    #[test]
    fn classify_reason_connection_refused() {
        assert_eq!(
            classify_failure_reason("connection refused"),
            FailureClass::TransientInfra
        );
    }

    #[test]
    fn classify_reason_turn_limit() {
        assert_eq!(
            classify_failure_reason("exceeded turn limit of 10"),
            FailureClass::BudgetExhausted
        );
    }

    #[test]
    fn classify_reason_token_limit() {
        assert_eq!(
            classify_failure_reason("token limit reached"),
            FailureClass::BudgetExhausted
        );
    }

    #[test]
    fn classify_reason_cancel() {
        assert_eq!(
            classify_failure_reason("operation cancelled by user"),
            FailureClass::Canceled
        );
    }

    #[test]
    fn classify_reason_scope_violation() {
        assert_eq!(
            classify_failure_reason("scope violation detected"),
            FailureClass::Structural
        );
    }

    #[test]
    fn classify_reason_default_deterministic() {
        assert_eq!(
            classify_failure_reason("invalid configuration parameter"),
            FailureClass::Deterministic
        );
    }
}
