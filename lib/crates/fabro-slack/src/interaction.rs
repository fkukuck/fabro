use fabro_interview::Answer;
use serde_json::Value;

/// Parses a Slack interaction payload and returns (question_id, Answer).
///
/// Action IDs follow the format `{question_id}:{action}` as set by `blocks::question_to_blocks`.
pub fn parse_interaction(payload: &Value) -> Option<(String, Answer)> {
    if payload["type"].as_str()? != "block_actions" {
        return None;
    }

    let action = payload["actions"].as_array()?.first()?;
    let action_id = action["action_id"].as_str()?;
    let (question_id, action_key) = action_id.split_once(':')?;

    let action_type = action["type"].as_str().unwrap_or("button");

    let answer = match action_type {
        "button" => match action_key {
            "yes" => Answer::yes(),
            "no" => Answer::no(),
            "submit" => extract_checkbox_selections(question_id, payload),
            key => {
                let value = action["value"].as_str().unwrap_or(key);
                Answer::text(value.to_string())
            }
        },
        "checkboxes" => {
            // Ignore checkbox toggle events — wait for Submit button
            return None;
        }
        "plain_text_input" => {
            let value = action["value"].as_str()?;
            Answer::text(value.to_string())
        }
        _ => return None,
    };

    Some((question_id.to_string(), answer))
}

/// Extract selected checkbox values from `payload.state.values`.
/// The checkbox block has block_id `{question_id}:checkboxes` and
/// action_id `{question_id}:select`.
fn extract_checkbox_selections(question_id: &str, payload: &Value) -> Answer {
    let block_id = format!("{question_id}:checkboxes");
    let action_id = format!("{question_id}:select");

    let selected = payload["state"]["values"][&block_id][&action_id]["selected_options"].as_array();

    match selected {
        Some(options) if !options.is_empty() => {
            let values: Vec<String> = options
                .iter()
                .filter_map(|opt| opt["value"].as_str().map(String::from))
                .collect();
            Answer::text(values.join(", "))
        }
        _ => Answer::skipped(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fabro_interview::AnswerValue;

    #[test]
    fn parse_yes_button_click() {
        let payload = serde_json::json!({
            "type": "block_actions",
            "actions": [{
                "action_id": "q-1:yes",
                "type": "button",
                "value": "yes"
            }]
        });
        let result = parse_interaction(&payload).unwrap();
        assert_eq!(result.0, "q-1");
        assert_eq!(result.1.value, AnswerValue::Yes);
    }

    #[test]
    fn parse_no_button_click() {
        let payload = serde_json::json!({
            "type": "block_actions",
            "actions": [{
                "action_id": "q-2:no",
                "type": "button",
                "value": "no"
            }]
        });
        let result = parse_interaction(&payload).unwrap();
        assert_eq!(result.0, "q-2");
        assert_eq!(result.1.value, AnswerValue::No);
    }

    #[test]
    fn parse_multiple_choice_button() {
        let payload = serde_json::json!({
            "type": "block_actions",
            "actions": [{
                "action_id": "q-3:rs",
                "type": "button",
                "value": "rs"
            }]
        });
        let result = parse_interaction(&payload).unwrap();
        assert_eq!(result.0, "q-3");
        assert_eq!(result.1.value, AnswerValue::Text("rs".to_string()));
    }

    #[test]
    fn checkbox_toggle_is_ignored() {
        let payload = serde_json::json!({
            "type": "block_actions",
            "actions": [{
                "action_id": "q-5:select",
                "type": "checkboxes",
                "selected_options": [
                    { "value": "a" },
                    { "value": "b" }
                ]
            }]
        });
        assert!(parse_interaction(&payload).is_none());
    }

    #[test]
    fn submit_button_reads_checkbox_state() {
        let payload = serde_json::json!({
            "type": "block_actions",
            "actions": [{
                "action_id": "q-5:submit",
                "type": "button",
                "value": "submit"
            }],
            "state": {
                "values": {
                    "q-5:checkboxes": {
                        "q-5:select": {
                            "type": "checkboxes",
                            "selected_options": [
                                { "value": "auth" },
                                { "value": "billing" }
                            ]
                        }
                    }
                }
            }
        });
        let result = parse_interaction(&payload).unwrap();
        assert_eq!(result.0, "q-5");
        assert_eq!(
            result.1.value,
            AnswerValue::Text("auth, billing".to_string())
        );
    }

    #[test]
    fn submit_button_with_no_checkboxes_selected() {
        let payload = serde_json::json!({
            "type": "block_actions",
            "actions": [{
                "action_id": "q-5:submit",
                "type": "button",
                "value": "submit"
            }],
            "state": {
                "values": {
                    "q-5:checkboxes": {
                        "q-5:select": {
                            "type": "checkboxes",
                            "selected_options": []
                        }
                    }
                }
            }
        });
        let result = parse_interaction(&payload).unwrap();
        assert_eq!(result.0, "q-5");
        assert_eq!(result.1.value, AnswerValue::Skipped);
    }

    #[test]
    fn parse_plain_text_input() {
        let payload = serde_json::json!({
            "type": "block_actions",
            "actions": [{
                "action_id": "q-6:input",
                "type": "plain_text_input",
                "value": "https://github.com/org/repo"
            }]
        });
        let result = parse_interaction(&payload).unwrap();
        assert_eq!(result.0, "q-6");
        assert_eq!(
            result.1.value,
            AnswerValue::Text("https://github.com/org/repo".to_string())
        );
    }

    #[test]
    fn returns_none_for_empty_actions() {
        let payload = serde_json::json!({
            "type": "block_actions",
            "actions": []
        });
        assert!(parse_interaction(&payload).is_none());
    }

    #[test]
    fn returns_none_for_unknown_type() {
        let payload = serde_json::json!({
            "type": "view_submission"
        });
        assert!(parse_interaction(&payload).is_none());
    }

    #[test]
    fn returns_none_for_malformed_action_id() {
        let payload = serde_json::json!({
            "type": "block_actions",
            "actions": [{
                "action_id": "no-colon",
                "type": "button",
                "value": "yes"
            }]
        });
        assert!(parse_interaction(&payload).is_none());
    }
}
