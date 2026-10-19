# Triggers reference

Replace `<env>` with the environment name and `<delegate-id>` with the Linear
delegate id. Add the actions under `actions` and the triggers under
`triggers`. If a key already exists, ask the user before you replace it.

New factory actions use Claude Opus 5.5. The user can change the model or
remove it. Removing it uses the platform default instead.

## Issue to PR with Linear

```json
"actions": {
  "implement-linear": {
    "skill": "tessl-labs/factory#implement-linear",
    "environment": "<env>",
    "agent": "claude",
    "model": "claude-opus-5-5"
  }
},
"triggers": {
  "linear-issue-delegated": {
    "on": ["linear:Issue.create", "linear:Issue.update"],
    "when": {
      "event.data.delegateId": "<delegate-id>",
      "event.data.state.type": "unstarted"
    },
    "concurrency": {
      "keys": [{ "key": "implement:{{ event.data.id }}", "limit": 1 }]
    },
    "action": "implement-linear"
  }
}
```

## Issue to PR with GitHub Issues

```json
"actions": {
  "implement-github-issue": {
    "skill": "tessl-labs/factory#implement-github-issue",
    "environment": "<env>",
    "agent": "claude",
    "model": "claude-opus-5-5"
  }
},
"triggers": {
  "github-issue-labelled": {
    "on": ["github:issues.labeled"],
    "when": { "event.label.name": "tessl" },
    "concurrency": {
      "keys": [
        {
          "key": "implement:{{ event.repository.id }}#{{ event.issue.number }}",
          "limit": 1
        }
      ]
    },
    "action": "implement-github-issue"
  }
}
```

## PR shepherding

The implement skills open pull requests with the `tessl` label on a `tessl/`
branch. `shepherd-pr-review` and `shepherd-pr-review-comment` match both the
label and the branch. `shepherd-pr-comment` matches only the label; the
shepherd then stops without acting on a pull request that is not on a `tessl/`
branch. `shepherd-checks-failed` and `shepherd-checks-passed` match only the
`tessl/` branch, not the label, so removing the label does not stop them.
To turn the shepherd off, remove the `shepherd` action and all five of its
triggers. That prevents new rounds; it does not cancel one in progress.
Comments from bots do not start a round.

```json
"actions": {
  "shepherd": {
    "skill": "tessl-labs/factory#shepherd",
    "environment": "<env>",
    "agent": "claude",
    "model": "claude-opus-5-5"
  }
},
"triggers": {
  "shepherd-pr-review": {
    "on": ["github:pull_request_review.submitted"],
    "when": {
      "event.pull_request.labels.*.name": { "any": { "equals": "tessl" } },
      "event.pull_request.head.ref": { "matches": "^tessl/" }
    },
    "concurrency": {
      "debounce": "90s",
      "keys": [{ "key": "shepherd:{{ event.pull_request.number }}", "limit": 1 }]
    },
    "action": "shepherd"
  },
  "shepherd-pr-review-comment": {
    "on": ["github:pull_request_review_comment.created"],
    "when": {
      "event.pull_request.labels.*.name": { "any": { "equals": "tessl" } },
      "event.pull_request.head.ref": { "matches": "^tessl/" },
      "event.sender.type": { "not": { "equals": "Bot" } }
    },
    "concurrency": {
      "debounce": "90s",
      "keys": [{ "key": "shepherd:{{ event.pull_request.number }}", "limit": 1 }]
    },
    "action": "shepherd"
  },
  "shepherd-pr-comment": {
    "on": ["github:issue_comment.created"],
    "when": {
      "isPullRequest": "true",
      "event.issue.labels.*.name": { "any": { "equals": "tessl" } },
      "event.sender.type": { "not": { "equals": "Bot" } }
    },
    "concurrency": {
      "debounce": "90s",
      "keys": [{ "key": "shepherd:{{ event.issue.number }}", "limit": 1 }]
    },
    "action": "shepherd"
  },
  "shepherd-checks-failed": {
    "on": ["github:check_run.completed"],
    "when": {
      "event.check_run.pull_requests.0.number": { "exists": true },
      "event.check_run.check_suite.head_branch": { "matches": "^tessl/" },
      "event.check_run.conclusion": {
        "in": ["failure", "timed_out", "startup_failure", "action_required"]
      }
    },
    "concurrency": {
      "debounce": "90s",
      "keys": [
        { "key": "shepherd:{{ event.check_run.pull_requests.0.number }}", "limit": 1 }
      ]
    },
    "action": "shepherd"
  },
  "shepherd-checks-passed": {
    "on": ["github:check_suite.completed"],
    "when": {
      "event.check_suite.pull_requests.0.number": { "exists": true },
      "event.check_suite.head_branch": { "matches": "^tessl/" },
      "event.check_suite.conclusion": "success"
    },
    "concurrency": {
      "debounce": "90s",
      "keys": [
        { "key": "shepherd:{{ event.check_suite.pull_requests.0.number }}", "limit": 1 }
      ]
    },
    "action": "shepherd"
  }
}
```

A Linear issue to PR setup with shepherding also needs `--linear-access` on
the environment, because the shepherd reads the Linear issue.
