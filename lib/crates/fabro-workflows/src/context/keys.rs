//! Static context key constants and helper functions for dynamic keys.
//!
//! All context keys used across the engine, handlers, and preamble are
//! defined here to prevent typos and improve discoverability.

// --- Top-level keys ---
pub const CURRENT_NODE: &str = "current_node";
pub const OUTCOME: &str = "outcome";
pub const FAILURE_CLASS: &str = "failure_class";
pub const FAILURE_SIGNATURE: &str = "failure_signature";
pub const PREFERRED_LABEL: &str = "preferred_label";
pub const LAST_STAGE: &str = "last_stage";
pub const LAST_RESPONSE: &str = "last_response";

// --- graph.* keys ---
pub const GRAPH_GOAL: &str = "graph.goal";

// --- internal.* keys ---
pub const INTERNAL_RUN_ID: &str = "internal.run_id";
pub const INTERNAL_WORK_DIR: &str = "internal.work_dir";
pub const INTERNAL_FIDELITY: &str = "internal.fidelity";
pub const INTERNAL_THREAD_ID: &str = "internal.thread_id";
pub const INTERNAL_NODE_VISIT_COUNT: &str = "internal.node_visit_count";
pub const INTERNAL_PARENT_PREAMBLE: &str = "internal.parent_preamble";

// --- current.* keys ---
pub const CURRENT_PREAMBLE: &str = "current.preamble";

// --- command.* keys ---
pub const COMMAND_OUTPUT: &str = "command.output";
pub const COMMAND_STDERR: &str = "command.stderr";

// --- human.gate.* keys ---
pub const HUMAN_GATE_SELECTED: &str = "human.gate.selected";
pub const HUMAN_GATE_LABEL: &str = "human.gate.label";
pub const HUMAN_GATE_TEXT: &str = "human.gate.text";

// --- parallel.* keys ---
pub const PARALLEL_RESULTS: &str = "parallel.results";
pub const PARALLEL_BRANCH_COUNT: &str = "parallel.branch_count";
pub const PARALLEL_FAN_IN_BEST_ID: &str = "parallel.fan_in.best_id";
pub const PARALLEL_FAN_IN_BEST_OUTCOME: &str = "parallel.fan_in.best_outcome";
pub const PARALLEL_FAN_IN_BEST_HEAD_SHA: &str = "parallel.fan_in.best_head_sha";

// --- Prefix constants (for filtering and dynamic keys) ---
pub const GRAPH_PREFIX: &str = "graph.";
pub const INTERNAL_PREFIX: &str = "internal.";
pub const CURRENT_PREFIX: &str = "current";
pub const THREAD_PREFIX: &str = "thread.";
pub const RESPONSE_PREFIX: &str = "response.";
pub const INTERNAL_RETRY_COUNT_PREFIX: &str = "internal.retry_count.";

// --- Helper functions for dynamic keys ---

#[must_use]
pub fn response_key(node_id: &str) -> String {
    format!("{RESPONSE_PREFIX}{node_id}")
}

#[must_use]
pub fn thread_current_node_key(thread_id: &str) -> String {
    format!("{THREAD_PREFIX}{thread_id}.current_node")
}

#[must_use]
pub fn graph_attr_key(attr: &str) -> String {
    format!("{GRAPH_PREFIX}{attr}")
}

#[must_use]
pub fn retry_count_key(node_id: &str) -> String {
    format!("{INTERNAL_RETRY_COUNT_PREFIX}{node_id}")
}

/// Returns `true` for engine-internal keys that should not propagate from child
/// to parent workflow contexts.
#[must_use]
pub fn is_engine_internal_key(key: &str) -> bool {
    key.starts_with(INTERNAL_PREFIX)
        || key.starts_with(GRAPH_PREFIX)
        || key.starts_with(THREAD_PREFIX)
        || key.starts_with(CURRENT_PREFIX)
}

pub use fabro_graphviz::Fidelity;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn response_key_formats_correctly() {
        assert_eq!(response_key("plan"), "response.plan");
    }

    #[test]
    fn thread_current_node_key_formats_correctly() {
        assert_eq!(thread_current_node_key("main"), "thread.main.current_node");
    }

    #[test]
    fn graph_attr_key_formats_correctly() {
        assert_eq!(graph_attr_key("goal"), "graph.goal");
    }

    #[test]
    fn retry_count_key_formats_correctly() {
        assert_eq!(retry_count_key("plan"), "internal.retry_count.plan");
    }

    #[test]
    fn is_engine_internal_key_classifies_correctly() {
        // Keys that ARE engine-internal (should not propagate)
        assert!(is_engine_internal_key("internal.run_id"));
        assert!(is_engine_internal_key("internal.fidelity"));
        assert!(is_engine_internal_key("internal.parent_preamble"));
        assert!(is_engine_internal_key("graph.goal"));
        assert!(is_engine_internal_key("thread.main.current_node"));
        assert!(is_engine_internal_key("current.preamble"));
        assert!(is_engine_internal_key("current_node"));

        // Keys that are NOT engine-internal (should propagate)
        assert!(!is_engine_internal_key("response.plan"));
        assert!(!is_engine_internal_key("command.output"));
        assert!(!is_engine_internal_key("outcome"));
        assert!(!is_engine_internal_key("last_stage"));
        assert!(!is_engine_internal_key("review.result"));
        assert!(!is_engine_internal_key("user.name"));
    }
}
