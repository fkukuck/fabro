//! Command execution over the data plane's one-shot shell endpoint.
//!
//! `executeShellCommand` runs a shell string to completion and answers with
//! its output as JSON text: no streaming, no stdin, no signals, and no
//! binary output. Each command is therefore launched detached inside the
//! sandbox under a small POSIX `sh` runner, in a directory of its own:
//!
//! - the launch claims the directory with an atomic `mkdir`, so a launch
//!   retried after a dropped connection finds the claim and never runs the
//!   command a second time;
//! - the runner writes stdout and stderr to files and the exit status to
//!   `status` when the command ends, and records its process id, which is also
//!   the process group and session it leads;
//! - the driver polls with read-only calls that return each stream's new bytes
//!   as base64, from the offsets it has already delivered, so a failed poll is
//!   simply repeated;
//! - a stop sends the signal to the runner's whole process group and session,
//!   so a command's children stop with it;
//! - streamed stdin is written as numbered chunk files that a feeder in the
//!   runner copies into the command's stdin in order.
//!
//! Commands run concurrently: each has its own directory and process, and
//! no lock is held across a poll. When the outcome cannot be established
//! (the sandbox stopped under a running command, or the service stayed
//! unreachable), the result is [`Error::Incomplete`], never a guessed exit.

use std::fmt::Write as _;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use sandbox_driver::{
    Error, Exec, ExecControls, ExecResult, ExecSpec, ExecStreamingResult, IncompleteOperation,
    OutputCaptureBuffer, OutputSanitizer, OutputStream, ProviderError, ResourceKind, Result,
    StopLevel, Termination, run_with_stop_grace, stop_signal,
};
use tokio::io::AsyncReadExt;
use tokio::task::JoinHandle;

use crate::client::{AcaClient, Body, Replay, SendError, kind};
use crate::models::WireShellResult;

/// Where every command's directory lives inside the sandbox.
pub(crate) const EXEC_ROOT: &str = "/tmp/.fabro-aca/exec";
/// Raw bytes one poll returns per stream; more is fetched at once.
const POLL_CHUNK_BYTES: usize = 1024 * 1024;
/// How long a launch waits in the sandbox for a quick command to finish,
/// so the common short command takes a single round trip.
const LAUNCH_WAIT_TENTHS: u32 = 15;
const FIRST_POLL: Duration = Duration::from_millis(100);
const MAX_POLL: Duration = Duration::from_secs(1);
/// How long polling may keep failing before the outcome is reported as
/// unknown.
const POLL_FAILURE_BUDGET: Duration = Duration::from_secs(120);
/// How long after a KILL the runner may take to disappear before the
/// command is reported killed without a status.
const KILL_SETTLE: Duration = Duration::from_secs(10);
/// Raw bytes per streamed stdin chunk file.
const STDIN_CHUNK_BYTES: usize = 256 * 1024;
/// The largest inline launch; a bigger spec is uploaded as a file first.
const MAX_INLINE_SPEC: usize = 48 * 1024;

/// The runner: `$1` is the command's directory. It sources `spec.sh` for
/// the argv (`set --`), the environment and the working directory, then
/// runs the command with its streams in files and writes `status` when it
/// ends. The trap keeps the runner alive through a TERM sent to its group
/// so it can still record how the command ended; a handler, unlike an
/// ignored signal, is reset in the command it starts.
const RUNNER: &str = r#"D=$1
echo $$ > "$D/pid.tmp" && mv "$D/pid.tmp" "$D/pid"
trap 'fabro_term=1' TERM
. "$D/spec.sh"
fabro_in=/dev/null
fabro_feeder=
if [ -f "$D/stdin" ]; then
  fabro_in=$D/stdin
elif [ -d "$D/in" ]; then
  mkfifo "$D/in.fifo"
  (
    trap - TERM
    i=0
    while :; do
      if [ -e "$D/in/$i.ok" ]; then
        cat "$D/in/$i" 2>/dev/null
        rm -f "$D/in/$i" "$D/in/$i.ok"
        i=$((i + 1))
      elif [ -e "$D/in/eof" ]; then
        break
      else
        sleep 0.05 2>/dev/null || sleep 1
      fi
    done
  ) > "$D/in.fifo" &
  fabro_feeder=$!
  fabro_in=$D/in.fifo
fi
(
  trap - TERM
  cd "$fabro_cwd" 2>/dev/null || { echo "sandbox-driver: cannot enter $fabro_cwd" >&2; exit 126; }
  exec "$@"
) < "$fabro_in" > "$D/out" 2> "$D/err" &
fabro_child=$!
wait "$fabro_child"
fabro_status=$?
while kill -0 "$fabro_child" 2>/dev/null; do
  wait "$fabro_child"
  fabro_status=$?
done
[ -n "$fabro_feeder" ] && kill "$fabro_feeder" 2>/dev/null
echo "$fabro_status" > "$D/status.tmp" && mv "$D/status.tmp" "$D/status"
"#;

/// Prints the command's state from the given offsets: `S <status>`,
/// `A <alive>`, then each stream's new bytes as base64. Liveness is read
/// before the status, so a runner found dead has written its status if it
/// ever will; output is read last, so output read after a status is
/// complete.
fn poll_script(dir: &str, out_offset: u64, err_offset: u64) -> String {
    format!(
        r#"D={dir}
alive=0; pid=$(cat "$D/pid" 2>/dev/null); [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null && alive=1
[ -d "$D" ] || alive=gone
st=; [ -f "$D/status" ] && st=$(cat "$D/status")
printf 'S %s\nA %s\n' "$st" "$alive"
printf 'O '; tail -c +{out} "$D/out" 2>/dev/null | head -c {max} | base64 | tr -d '\n'; printf '\n'
printf 'E '; tail -c +{err} "$D/err" 2>/dev/null | head -c {max} | base64 | tr -d '\n'; printf '\n'
"#,
        dir = shell_quote(dir),
        out = out_offset + 1,
        err = err_offset + 1,
        max = POLL_CHUNK_BYTES,
    )
}

/// One poll's answer.
#[derive(Debug, Default, PartialEq, Eq)]
struct PollState {
    launched: Option<bool>,
    status:   Option<i32>,
    alive:    Alive,
    stdout:   Vec<u8>,
    stderr:   Vec<u8>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Alive {
    #[default]
    Unknown,
    Yes,
    No,
    /// The command's directory is gone: the sandbox lost its `/tmp`.
    Gone,
}

fn parse_poll(text: &str) -> Result<PollState> {
    let mut state = PollState::default();
    for line in text.lines() {
        let (tag, rest) = line.split_once(' ').unwrap_or((line, ""));
        match tag {
            "L" => state.launched = Some(rest.trim() == "launched"),
            "S" => state.status = rest.trim().parse().ok(),
            "A" => {
                state.alive = match rest.trim() {
                    "1" => Alive::Yes,
                    "0" => Alive::No,
                    "gone" => Alive::Gone,
                    _ => Alive::Unknown,
                }
            }
            "O" => state.stdout = decode(rest)?,
            "E" => state.stderr = decode(rest)?,
            _ => {}
        }
    }
    Ok(state)
}

fn decode(text: &str) -> Result<Vec<u8>> {
    BASE64.decode(text.trim()).map_err(|error| {
        Error::Provider(ProviderError::new(
            kind(),
            format!("command output arrived garbled: {error}"),
        ))
    })
}

/// An outcome that could not be established; cleanup is never confirmed.
fn incomplete(
    operation: impl Into<String>,
    output_abandoned: bool,
    stop_acknowledged: bool,
    termination_confirmed: bool,
) -> Error {
    let mut incomplete = IncompleteOperation::new(operation);
    incomplete.output_abandoned = output_abandoned;
    incomplete.stop_acknowledged = stop_acknowledged;
    incomplete.termination_confirmed = termination_confirmed;
    Error::Incomplete(incomplete)
}

/// Single-quotes `value` for a POSIX shell.
pub(crate) fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
}

/// The sourced part of a launch: the argv, the environment, and the
/// working directory, all quoted.
fn spec_script(spec: &ExecSpec, default_cwd: &str) -> String {
    let mut script = String::from("set --");
    script.push(' ');
    script.push_str(&shell_quote(&spec.program));
    for arg in &spec.args {
        script.push(' ');
        script.push_str(&shell_quote(arg));
    }
    script.push('\n');
    for (key, value) in spec.launch_env().iter() {
        let _ = writeln!(script, "export {}", shell_quote(&format!("{key}={value}")));
    }
    let cwd = spec.working_dir.as_deref().unwrap_or(default_cwd);
    let _ = writeln!(script, "fabro_cwd={}", shell_quote(cwd));
    script
}

/// The launch: claim the directory, start the runner in its own session,
/// wait briefly for a quick command, then report like a poll.
fn launch_script(dir: &str, spec_inline: Option<&str>, stdin_inline: Option<&[u8]>) -> String {
    let quoted = shell_quote(dir);
    let mut script = format!(
        "D={quoted}\nmkdir -p {root}\n",
        root = shell_quote(EXEC_ROOT)
    );
    // Sweep directories older than a day; never this one.
    let _ = writeln!(
        script,
        "( find {root} -mindepth 1 -maxdepth 1 -mmin +1440 -exec rm -rf {{}} + ) >/dev/null 2>&1 &",
        root = shell_quote(EXEC_ROOT)
    );
    script.push_str("mkdir -p \"$D\"\n");
    script.push_str("if mkdir \"$D/run\" 2>/dev/null; then\n");
    if let Some(spec) = spec_inline {
        let _ = writeln!(
            script,
            "  printf '%s' '{}' | base64 -d > \"$D/spec.sh\"",
            BASE64.encode(spec)
        );
    }
    if let Some(stdin) = stdin_inline {
        let _ = writeln!(
            script,
            "  printf '%s' '{}' | base64 -d > \"$D/stdin\"",
            BASE64.encode(stdin)
        );
    }
    let _ = writeln!(
        script,
        "  printf '%s' '{}' | base64 -d > \"$D/runner.sh\"",
        BASE64.encode(RUNNER)
    );
    script.push_str(
        "  if command -v setsid >/dev/null 2>&1; then\n    nohup setsid sh \"$D/runner.sh\" \"$D\" </dev/null >/dev/null 2>&1 &\n  else\n    ( set -m; nohup sh \"$D/runner.sh\" \"$D\" </dev/null >/dev/null 2>&1 & )\n  fi\n  echo 'L launched'\nelse\n  echo 'L exists'\nfi\n",
    );
    let _ = writeln!(
        script,
        "i=0; while [ ! -f \"$D/status\" ] && [ $i -lt {LAUNCH_WAIT_TENTHS} ]; do sleep 0.1 2>/dev/null || break; i=$((i + 1)); done"
    );
    script.push_str(&poll_script(dir, 0, 0));
    script
}

fn signal_script(dir: &str, signal: &str) -> String {
    format!(
        "D={dir}\npid=$(cat \"$D/pid\" 2>/dev/null)\n[ -n \"$pid\" ] || exit 0\nkill -s {signal} -- -\"$pid\" 2>/dev/null\ncommand -v pkill >/dev/null 2>&1 && pkill -{signal} -s \"$pid\" 2>/dev/null\nkill -s {signal} \"$pid\" 2>/dev/null\nexit 0\n",
        dir = shell_quote(dir)
    )
}

/// Wraps a script so any shell runs it: base64 carries no quotes.
fn wrapped(script: &str) -> String {
    format!("printf '%s' '{}' | base64 -d | sh", BASE64.encode(script))
}

/// The exec facet of one sandbox.
#[derive(Clone)]
pub(crate) struct AcaExec {
    client:            AcaClient,
    sandbox_id:        String,
    working_directory: String,
}

impl AcaExec {
    pub(crate) fn new(client: AcaClient, sandbox_id: String, working_directory: String) -> Self {
        Self {
            client,
            sandbox_id,
            working_directory,
        }
    }

    fn sandbox_path(&self, suffix: &str) -> String {
        format!("/sandboxes/{}{suffix}", self.sandbox_id)
    }

    /// Runs a shell string through the one-shot endpoint and returns its
    /// stdout. Only for the driver's own idempotent scripts.
    pub(crate) async fn shell(&self, script: &str, replay: Replay) -> Result<String> {
        let body = serde_json::json!({
            "command": wrapped(script),
            "workingDirectory": "/",
        });
        let send = || {
            self.client.data_json::<WireShellResult>(
                reqwest::Method::POST,
                &self.sandbox_path("/executeShellCommand"),
                &[],
                Body::Json(body.clone()),
                replay,
            )
        };
        let result = match send().await {
            // ACA suspends an idle sandbox and refuses commands until it is
            // resumed. The refused command never ran, so it is sent again.
            Err(error) if error.status() == Some(409) => {
                tracing::debug!(sandbox = %self.sandbox_id, "resuming a suspended ACA sandbox");
                crate::sandbox::resume(&self.client, &self.sandbox_id).await?;
                send().await
            }
            other => other,
        }
        .map_err(|error| error.into_error(ResourceKind::Sandbox, &self.sandbox_id))?;
        if result.exit_code.unwrap_or(0) != 0 {
            return Err(Error::Provider(ProviderError::new(
                kind(),
                format!(
                    "the sandbox's shell failed (exit {}): {}",
                    result.exit_code.unwrap_or(-1),
                    result.stderr.trim()
                ),
            )));
        }
        Ok(result.stdout)
    }

    async fn upload(&self, path: &str, bytes: Vec<u8>) -> Result<()> {
        self.client
            .data(
                reqwest::Method::PUT,
                &self.sandbox_path("/files"),
                &[("path", path), ("createDirs", "true")],
                Body::Bytes(bytes),
                Replay::Safe,
            )
            .await
            .map(|_| ())
            .map_err(|error| error.into_error(ResourceKind::File, path))
    }

    /// Launches the command, retrying a launch whose answer was lost: the
    /// directory claim makes a repeat find the first.
    async fn launch(&self, dir: &str, spec: &ExecSpec) -> Result<PollState> {
        let sourced = spec_script(spec, &self.working_directory);
        let spec_inline = if sourced.len() <= MAX_INLINE_SPEC {
            Some(sourced.as_str())
        } else {
            self.upload(&format!("{dir}/spec.sh"), sourced.clone().into_bytes())
                .await?;
            None
        };
        let stdin_inline = match spec.stdin.as_deref() {
            Some(bytes) if bytes.len() <= MAX_INLINE_SPEC => Some(bytes),
            Some(bytes) => {
                self.upload(&format!("{dir}/stdin"), bytes.to_vec()).await?;
                None
            }
            None => None,
        };
        let script = launch_script(dir, spec_inline, stdin_inline);
        let mut attempts = 0;
        loop {
            attempts += 1;
            match self.shell(&script, Replay::Never).await {
                Ok(text) => return parse_poll(&text),
                Err(Error::Transport(error)) if attempts < 4 => {
                    tracing::debug!(%error, "retrying an ACA command launch");
                    tokio::time::sleep(Duration::from_millis(500 * attempts)).await;
                }
                Err(Error::Transport(_)) => {
                    return Err(incomplete(
                        "launching a command in the ACA sandbox",
                        true,
                        false,
                        false,
                    ));
                }
                Err(error) => return Err(error),
            }
        }
    }

    async fn poll(&self, dir: &str, out: u64, err: u64) -> Result<PollState> {
        let text = self
            .shell(&poll_script(dir, out, err), Replay::Safe)
            .await?;
        parse_poll(&text)
    }

    async fn signal(&self, dir: &str, signal: &str) -> Result<()> {
        self.shell(&signal_script(dir, signal), Replay::Safe)
            .await
            .map(|_| ())
    }

    /// Removes the command's files, keeping the claim so a late repeat of
    /// the launch still finds it. Best effort, off the caller's path.
    fn clean_up(&self, dir: String) {
        let exec = self.clone();
        tokio::spawn(async move {
            let script = format!(
                "D={}\nrm -rf \"$D/out\" \"$D/err\" \"$D/in\" \"$D/in.fifo\" \"$D/stdin\" \"$D/spec.sh\" \"$D/runner.sh\"\n",
                shell_quote(&dir)
            );
            if let Err(error) = exec.shell(&script, Replay::Safe).await {
                tracing::debug!(%error, "ACA command cleanup failed");
            }
        });
    }

    /// Streams `controls.stdin` into the command as chunk files, then the
    /// end-of-input marker.
    fn feed_stdin(&self, dir: &str, controls: &ExecControls) -> Option<JoinHandle<Result<()>>> {
        let source = controls.stdin.as_ref()?;
        let mut reader = source.take()?;
        let exec = self.clone();
        let dir = dir.to_owned();
        Some(tokio::spawn(async move {
            let mut index = 0_u64;
            let mut buffer = vec![0_u8; STDIN_CHUNK_BYTES];
            loop {
                let count = reader
                    .read(&mut buffer)
                    .await
                    .map_err(|error| Error::io("reading the command's stdin", error))?;
                if count == 0 {
                    break;
                }
                exec.upload(&format!("{dir}/in/{index}"), buffer[..count].to_vec())
                    .await?;
                exec.upload(&format!("{dir}/in/{index}.ok"), Vec::new())
                    .await?;
                index += 1;
            }
            exec.upload(&format!("{dir}/in/eof"), Vec::new()).await
        }))
    }

    async fn run_signals(
        &self,
        spec: &ExecSpec,
        controls: ExecControls,
    ) -> Result<ExecStreamingResult> {
        let started = Instant::now();
        let dir = format!("{EXEC_ROOT}/{}", uuid::Uuid::new_v4().simple());
        // Streamed stdin needs its directory before the runner looks.
        if controls.stdin.is_some() {
            self.upload(&format!("{dir}/in/.keep"), Vec::new()).await?;
        }
        let mut state = self.launch(&dir, spec).await?;
        let stdin_task = self.feed_stdin(&dir, &controls);
        let mut stdout = Stream::new(&controls, spec);
        let mut stderr = Stream::new(&controls, spec);
        let mut stop: Option<Termination> = None;
        let mut killed_at: Option<Instant> = None;
        let mut interval = FIRST_POLL;
        let mut failing_since: Option<Instant> = None;
        let deadline = spec.timeout.map(|timeout| started + timeout);

        let outcome = loop {
            let progressed = !state.stdout.is_empty() || !state.stderr.is_empty();
            let full =
                state.stdout.len() >= POLL_CHUNK_BYTES || state.stderr.len() >= POLL_CHUNK_BYTES;
            if let Err(error) = stdout
                .deliver(OutputStream::Stdout, std::mem::take(&mut state.stdout))
                .await
            {
                tracing::debug!(%error, "output sink failed; killing the command");
                stop.get_or_insert(Termination::Cancelled);
                let _ = self.signal(&dir, "KILL").await;
                killed_at.get_or_insert_with(Instant::now);
            }
            if let Err(error) = stderr
                .deliver(OutputStream::Stderr, std::mem::take(&mut state.stderr))
                .await
            {
                tracing::debug!(%error, "output sink failed; killing the command");
                stop.get_or_insert(Termination::Cancelled);
                let _ = self.signal(&dir, "KILL").await;
                killed_at.get_or_insert_with(Instant::now);
            }

            if let (Some(status), false) = (state.status, full) {
                break Ok(Some(status));
            }
            if state.status.is_none() && matches!(state.alive, Alive::No | Alive::Gone) {
                // The runner is gone and wrote no status: the sandbox
                // stopped under it, or a KILL ended it.
                if killed_at.is_some() {
                    break Ok(None);
                } else {
                    break Err(incomplete(
                        format!("running `{}` in the ACA sandbox", spec.program),
                        state.alive == Alive::Gone,
                        false,
                        true,
                    ));
                }
            }
            if killed_at.is_some_and(|killed| killed.elapsed() > KILL_SETTLE) {
                break Ok(None);
            }

            let wait = if full || progressed {
                interval = FIRST_POLL;
                Duration::ZERO
            } else {
                let current = interval;
                interval = (interval * 2).min(MAX_POLL);
                current
            };
            let timer = async {
                match deadline {
                    Some(deadline) if stop.is_none() => {
                        tokio::time::sleep_until(deadline.into()).await;
                    }
                    _ => std::future::pending().await,
                }
            };
            tokio::select! {
                () = tokio::time::sleep(wait) => {}
                () = stop_signal(controls.term.as_ref()), if stop.is_none() => {
                    stop = Some(StopLevel::Term.termination());
                    self.signal(&dir, "TERM").await?;
                }
                () = stop_signal(controls.kill.as_ref()), if killed_at.is_none() => {
                    stop = Some(StopLevel::Kill.termination());
                    self.signal(&dir, "KILL").await?;
                    killed_at = Some(Instant::now());
                }
                () = timer => {
                    stop = Some(Termination::TimedOut);
                    self.signal(&dir, "KILL").await?;
                    killed_at = Some(Instant::now());
                }
            }

            match self.poll(&dir, stdout.raw_offset, stderr.raw_offset).await {
                Ok(next) => {
                    failing_since = None;
                    state = next;
                }
                Err(
                    error @ (Error::Transport(_) | Error::RateLimited { .. } | Error::Provider(_)),
                ) => {
                    let since = *failing_since.get_or_insert_with(Instant::now);
                    if since.elapsed() > POLL_FAILURE_BUDGET {
                        tracing::warn!(%error, "ACA command outcome could not be established");
                        break Err(incomplete(
                            format!("running `{}` in the ACA sandbox", spec.program),
                            true,
                            stop.is_some(),
                            false,
                        ));
                    }
                    state = PollState {
                        alive: Alive::Yes,
                        ..PollState::default()
                    };
                }
                Err(error) => break Err(error),
            }
        };

        if let Some(task) = stdin_task {
            task.abort();
        }
        let status = outcome?;
        self.clean_up(dir);
        let termination = stop.unwrap_or(match status {
            Some(_) => Termination::Exited,
            None => Termination::Killed,
        });
        let duration = started.elapsed();
        let mut result = ExecResult::from_shell_status(termination, status, duration);
        if status.is_none() && termination != Termination::Exited {
            result.signal = Some(9);
        }
        let (out_bytes, out_stats) = stdout.finish(OutputStream::Stdout).await;
        let (err_bytes, err_stats) = stderr.finish(OutputStream::Stderr).await;
        result.stdout = out_bytes;
        result.stderr = err_bytes;
        let mut streaming = ExecStreamingResult::new(result);
        streaming.streams_separated = true;
        streaming.live_streaming = true;
        streaming.stdout_capture = out_stats;
        streaming.stderr_capture = err_stats;
        Ok(streaming)
    }
}

/// One output stream: sanitized, delivered to the sink, and retained.
struct Stream {
    sink:       Option<sandbox_driver::OutputSink>,
    sanitizer:  OutputSanitizer,
    capture:    OutputCaptureBuffer,
    raw_offset: u64,
    sink_ok:    bool,
}

impl Stream {
    fn new(controls: &ExecControls, spec: &ExecSpec) -> Self {
        Self {
            sink:       controls.sink.clone(),
            sanitizer:  OutputSanitizer::new(spec.output_sanitization),
            capture:    OutputCaptureBuffer::new(controls.retained_output_limit),
            raw_offset: 0,
            sink_ok:    true,
        }
    }

    async fn deliver(&mut self, stream: OutputStream, raw: Vec<u8>) -> Result<()> {
        if raw.is_empty() {
            return Ok(());
        }
        self.raw_offset += raw.len() as u64;
        let clean = self.sanitizer.push(&raw);
        self.emit(stream, clean).await
    }

    async fn emit(&mut self, stream: OutputStream, clean: Vec<u8>) -> Result<()> {
        if clean.is_empty() {
            return Ok(());
        }
        self.capture.push(&clean);
        let Some(sink) = self.sink.as_ref().filter(|_| self.sink_ok) else {
            return Ok(());
        };
        if let Err(error) = sink(stream, clean).await {
            self.sink_ok = false;
            return Err(error);
        }
        Ok(())
    }

    async fn finish(mut self, stream: OutputStream) -> (Vec<u8>, sandbox_driver::CaptureStats) {
        let rest = self.sanitizer.finish();
        let _ = self.emit(stream, rest).await;
        self.capture.into_parts()
    }
}

#[async_trait]
impl Exec for AcaExec {
    async fn run(&self, spec: &ExecSpec) -> Result<ExecResult> {
        let streaming = self.run_streaming(spec, ExecControls::buffered()).await?;
        streaming.into_complete()
    }

    #[tracing::instrument(
        skip_all,
        fields(provider_kind = "aca", sandbox_id = %self.sandbox_id),
        err
    )]
    async fn run_streaming(
        &self,
        spec: &ExecSpec,
        controls: ExecControls,
    ) -> Result<ExecStreamingResult> {
        if controls.stdin.is_some() && spec.stdin.is_some() {
            return Err(Error::invalid_spec(
                "stdin",
                "a fixed stdin and a streamed stdin cannot both be given",
            ));
        }
        run_with_stop_grace(spec, controls, |spec, controls| async move {
            self.run_signals(&spec, controls).await
        })
        .await
    }
}

/// Maps a failed send to the driver error for the sandbox.
pub(crate) fn sandbox_error(error: SendError, sandbox_id: &str) -> Error {
    error.into_error(ResourceKind::Sandbox, sandbox_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoting_survives_single_quotes() {
        assert_eq!(shell_quote("it's"), r"'it'\''s'");
    }

    #[test]
    fn the_spec_script_quotes_every_word_and_blanks_bash_env_for_the_helper() {
        let spec = ExecSpec::bash("echo $HOME 'x'")
            .env_var("A", "b c")
            .working_dir("/work space");
        let script = spec_script(&spec, "/workspace");
        assert!(
            script.starts_with("set -- 'bash' '-c' 'echo $HOME '\\''x'\\'''"),
            "{script}"
        );
        assert!(script.contains("export 'A=b c'"), "{script}");
        assert!(script.contains("export 'BASH_ENV='"), "{script}");
        assert!(script.contains("fabro_cwd='/work space'"), "{script}");
    }

    #[test]
    fn a_poll_answer_decodes_both_streams_and_the_status() {
        let text = format!(
            "L exists\nS 3\nA 0\nO {}\nE {}\n",
            BASE64.encode(b"out\x00bin"),
            BASE64.encode(b"err")
        );
        let state = parse_poll(&text).unwrap();
        assert_eq!(state.launched, Some(false));
        assert_eq!(state.status, Some(3));
        assert_eq!(state.alive, Alive::No);
        assert_eq!(state.stdout, b"out\x00bin");
        assert_eq!(state.stderr, b"err");
    }

    #[test]
    fn a_running_command_has_no_status() {
        let state = parse_poll("S \nA 1\nO \nE \n").unwrap();
        assert_eq!(state.status, None);
        assert_eq!(state.alive, Alive::Yes);
        assert!(state.stdout.is_empty());
    }

    #[test]
    fn the_launch_claims_its_directory_before_starting_the_runner() {
        let script = launch_script("/tmp/.fabro-aca/exec/abc", Some("set -- 'true'\n"), None);
        let claim = script.find("mkdir \"$D/run\"").unwrap();
        let start = script.find("runner.sh\" \"$D\"").unwrap();
        assert!(claim < start);
        assert!(script.contains("L exists"));
    }

    #[test]
    fn the_wrapped_script_carries_no_quotes_but_its_own() {
        let text = wrapped("echo 'quoted' \"double\"");
        assert_eq!(text.matches('\'').count(), 4);
        assert!(!text.contains('"'));
    }
}
