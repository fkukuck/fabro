use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(crate) struct GithubIssueWebhookEvent {
    pub(crate) action:     String,
    pub(crate) issue:      GithubIssue,
    pub(crate) label:      Option<GithubLabel>,
    pub(crate) repository: GithubRepository,
}

#[derive(Debug, Deserialize)]
pub(crate) struct GithubIssue {
    pub(crate) number:       u64,
    pub(crate) title:        String,
    pub(crate) body:         Option<String>,
    pub(crate) html_url:     String,
    pub(crate) user:         GithubSender,
    #[serde(default)]
    pub(crate) labels:       Vec<GithubLabel>,
    pub(crate) pull_request: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct GithubLabel {
    pub(crate) name: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct GithubRepository {
    pub(crate) full_name:      String,
    pub(crate) default_branch: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct GithubSender {
    pub(crate) login: String,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct GithubIssueRunInputs {
    pub(crate) github_issue_url:      String,
    pub(crate) github_issue_number:   u64,
    pub(crate) github_issue_title:    String,
    pub(crate) github_issue_body:     String,
    pub(crate) github_issue_author:   String,
    pub(crate) github_repository:     String,
    pub(crate) github_default_branch: String,
    pub(crate) github_trigger_label:  String,
    pub(crate) github_delivery_id:    String,
}

pub(crate) fn parse_issue_event(body: &[u8]) -> serde_json::Result<GithubIssueWebhookEvent> {
    serde_json::from_slice(body)
}

impl GithubIssueWebhookEvent {
    pub(crate) fn added_label_name(&self) -> Option<&str> {
        self.label.as_ref().map(|label| label.name.as_str())
    }

    pub(crate) fn issue_label_names(&self) -> Vec<String> {
        self.issue
            .labels
            .iter()
            .map(|label| label.name.clone())
            .collect()
    }

    pub(crate) fn owner_repo(&self) -> Option<(&str, &str)> {
        self.repository.full_name.split_once('/')
    }

    pub(crate) fn is_pull_request(&self) -> bool {
        self.issue.pull_request.is_some()
    }

    pub(crate) fn run_inputs(
        &self,
        trigger_label: &str,
        delivery_id: &str,
    ) -> GithubIssueRunInputs {
        GithubIssueRunInputs {
            github_issue_url:      self.issue.html_url.clone(),
            github_issue_number:   self.issue.number,
            github_issue_title:    self.issue.title.clone(),
            github_issue_body:     self.issue.body.clone().unwrap_or_default(),
            github_issue_author:   self.issue.user.login.clone(),
            github_repository:     self.repository.full_name.clone(),
            github_default_branch: self.repository.default_branch.clone(),
            github_trigger_label:  trigger_label.to_owned(),
            github_delivery_id:    delivery_id.to_owned(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ISSUE_LABELED_PAYLOAD: &[u8] = br#"
{
  "action": "labeled",
  "label": { "name": "fabro" },
  "issue": {
    "number": 123,
    "title": "Fix production bug",
    "body": "The service returns 500.",
    "html_url": "https://github.com/owner/repo/issues/123",
    "user": { "login": "alice" },
    "labels": [
      { "name": "Bug" },
      { "name": "fabro" }
    ]
  },
  "repository": {
    "full_name": "owner/repo",
    "default_branch": "main",
    "html_url": "https://github.com/owner/repo"
  },
  "sender": { "login": "bob" }
}
"#;

    #[test]
    fn parses_issue_labeled_payload() {
        let event = parse_issue_event(ISSUE_LABELED_PAYLOAD).expect("payload parses");

        assert_eq!(event.action, "labeled");
        assert_eq!(event.added_label_name(), Some("fabro"));
        assert_eq!(event.issue_label_names(), vec!["Bug", "fabro"]);
        assert_eq!(event.owner_repo(), Some(("owner", "repo")));
    }

    #[test]
    fn builds_v1_workflow_inputs() {
        let event = parse_issue_event(ISSUE_LABELED_PAYLOAD).expect("payload parses");

        let inputs = event.run_inputs("fabro", "delivery-1");

        assert_eq!(inputs, GithubIssueRunInputs {
            github_issue_url:      "https://github.com/owner/repo/issues/123".to_owned(),
            github_issue_number:   123,
            github_issue_title:    "Fix production bug".to_owned(),
            github_issue_body:     "The service returns 500.".to_owned(),
            github_issue_author:   "alice".to_owned(),
            github_repository:     "owner/repo".to_owned(),
            github_default_branch: "main".to_owned(),
            github_trigger_label:  "fabro".to_owned(),
            github_delivery_id:    "delivery-1".to_owned(),
        });
    }
}
