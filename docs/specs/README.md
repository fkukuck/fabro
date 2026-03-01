# Arc

This repository contains [NLSpecs](#terminology) to build your own version of Arc to create your own software factory.

Although bringing your own agentic loop and unified LLM SDK is not required to build your own Arc, we highly recommend controlling the stack so you have a strong foundation.

## Specs

- [Arc Specification](./arc-spec.md)
- [Agent Specification](./coding-agent-loop-spec.md)
- [Unified LLM Client Specification](./unified-llm-spec.md)

## Building Arc

Supply the following prompt to a modern coding agent (Claude Code, Codex, OpenCode, Amp, Cursor, etc):

```
codeagent> Implement Arc as described by https://factory.strongdm.ai/
```

## Terminology

- **NLSpec** (Natural Language Spec): a human-readable spec intended to be  directly usable by coding agents to implement/validate behavior.
