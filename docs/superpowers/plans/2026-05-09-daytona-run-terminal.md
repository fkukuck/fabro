# Run Terminal Plan

## Summary

Build an embedded run terminal for Daytona and Docker sandboxes.

The run page gets a `Terminal` tab at `/runs/:id/terminal`. The browser uses `xterm.js`, connects to Fabro over WebSocket, and Fabro bridges bytes to a provider-specific terminal session for that run's sandbox. Daytona uses Daytona PTY. Docker uses an attached Docker exec with TTY. Keep the existing SSH access endpoint as a separate copyable external-access command for Daytona; do not build an SSH-over-WebSocket fallback.

## Interfaces

- Add WebSocket endpoint: `GET /api/v1/runs/{id}/terminal`.
- WebSocket auth uses existing Fabro web/session auth; reject invalid origins before upgrade.
- Browser protocol:
  - Client binary message: raw PTY stdin bytes.
  - Client text message: `{"type":"resize","cols":120,"rows":32}` or `{"type":"close"}`.
  - Server binary message: raw PTY output bytes.
  - Server text message: `{"type":"ready"}`, `{"type":"error","message":"..."}`, `{"type":"closed"}`.
- No OpenAPI/generated API client change for the WebSocket. Existing `POST /api/v1/runs/{id}/ssh` remains for "Copy SSH command".
- Add a small terminal-specific capability in `fabro-sandbox`, not full terminal support on the existing `Sandbox` trait:
  - `TerminalSize { cols: u16, rows: u16 }`
  - `TerminalSession` with `write_input`, `read_output`, `resize`, and `close`
  - concrete `DaytonaTerminalSession` and `DockerTerminalSession` implementations

## Key Changes

- Backend:
  - Enable Axum WebSocket support in `fabro-server`.
  - Add a focused terminal handler that loads the run sandbox record, reconnects the concrete provider, starts/restores it, opens a terminal session, bridges browser input/output, handles resize, and closes/kills the terminal session when the browser disconnects.
  - Add a Daytona PTY helper in `fabro-sandbox` that uses Daytona Toolbox APIs directly for create/connect/resize/kill because the pinned Rust SDK exposes PTY management but not a complete streaming handle.
  - Add a Docker terminal helper in `fabro-sandbox` that creates an attached Docker exec with `tty=true`, `attach_stdin=true`, `attach_stdout=true`, `attach_stderr=true`, starts it attached, writes browser input into Bollard's exec input writer, forwards exec output to the browser, and calls `resize_exec` on resize.
  - Docker disconnect cleanup should close the exec input/output and run the shell through a lightweight wrapper that records its PID so Fabro can terminate the shell process if the WebSocket drops.
  - Use the run sandbox working directory as the PTY `cwd`; set `TERM=xterm-256color` and `LANG=C.UTF-8`.
  - Keep provider credentials server-side only. Never send Daytona API keys, Daytona PTY URLs, Docker socket details, or provider connection handles to the browser.
- Frontend:
  - Add `@xterm/xterm` and `@xterm/addon-fit`.
  - Add `run-terminal.tsx`, mounted as `/runs/:id/terminal`, with full-height terminal layout.
  - Add a `Terminal` tab when the run has a sandbox id.
  - WebSocket URL uses `ws://` for `http://127.0.0.1` and `wss://` for HTTPS.
  - Add header actions: reconnect terminal, copy existing SSH command when the provider supports SSH, and connection status.
- Behavior:
  - Daytona and Docker are supported in v1. Local sandboxes show an unsupported-provider error.
  - PTY sessions are not persistent in v1. Closing or refreshing the tab starts a fresh shell.
  - Terminal input/output is not logged by Fabro.

## Test Plan

- Rust unit tests:
  - WebSocket message parser accepts valid resize and rejects malformed/oversized control messages.
  - Origin validation allows same-origin localhost and rejects cross-origin browser origins.
  - Daytona PTY helper builds the expected Toolbox REST/WebSocket URLs and auth headers.
  - Docker terminal helper creates exec options with TTY, stdin/stdout/stderr attached, workspace cwd, and terminal env.
- Server tests:
  - Unauthenticated terminal WebSocket upgrade is rejected.
  - Local or missing sandbox returns a clean unsupported/unavailable failure.
  - Daytona runs use the Daytona terminal adapter; Docker runs use the Docker terminal adapter.
  - On browser disconnect, the handler closes/kills the provider terminal session.
  - Resize messages call the provider resize operation with the latest cols/rows.
- Web tests:
  - Terminal tab appears for sandbox-backed runs.
  - Route opens `ws://127.0.0.1:port/...` on local HTTP and `wss://` on HTTPS.
  - Binary PTY output is written to xterm; keyboard input sends binary WebSocket messages.
  - Unsupported/error/closed states render without crashing.
- Manual acceptance:
  - Open Daytona-backed and Docker-backed runs, use `ls`, `pwd`, `vim`/`less`, Ctrl-C, resize the browser, refresh the tab, and confirm the old shell session is cleaned up.
  - Confirm "Copy SSH command" appears for Daytona and is absent/disabled for Docker.
  - Run `cargo nextest run -p fabro-server`, relevant `fabro-sandbox` tests, and `cd apps/fabro-web && bun test && bun run typecheck`.

## Assumptions

- Daytona PTY and Docker attached exec are the embedded-terminal transports for v1.
- `/api/v1/runs/{id}/ssh` stays as external access for local terminals and IDEs.
- WebSocket over plain `ws://127.0.0.1` is acceptable for local Fabro; hosted HTTPS deployments require `wss://`.
- Sources: Daytona PTY docs, Daytona SSH docs, Docker exec/Bollard APIs, and xterm.js docs.
