import { useState, useCallback, type FormEvent } from "react";
import { startPipeline, listPipelines, type PipelineStatusResponse } from "./api";
import { usePolling } from "./hooks";

const PLACEHOLDER = `digraph Example {
    graph [goal="Run a simple pipeline"]
    start [shape=Mdiamond]
    exit  [shape=Msquare]
    start -> exit
}`;

interface StartFormProps {
  onStart: (id: string) => void;
}

export function StartForm({ onStart }: StartFormProps) {
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const fetcher = useCallback(() => listPipelines(), []);
  const { data: pipelines } = usePolling(fetcher, 3000, true);

  async function handleSubmit(e: FormEvent<HTMLFormElement>) {
    e.preventDefault();
    const form = e.currentTarget;
    const dotSource = new FormData(form).get("dot_source") as string;
    if (!dotSource.trim()) return;

    setSubmitting(true);
    setError(null);

    try {
      const { id } = await startPipeline(dotSource);
      onStart(id);
    } catch (err) {
      setError(String(err));
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <form className="start-form" onSubmit={handleSubmit}>
      <label htmlFor="dot-source">DOT Graph Source</label>
      <textarea
        id="dot-source"
        name="dot_source"
        placeholder={PLACEHOLDER}
        defaultValue={PLACEHOLDER}
      />
      <div className="form-row">
        <button type="submit" className="btn-primary" disabled={submitting}>
          {submitting ? "Starting..." : "Start Pipeline"}
        </button>
        {error && <span className="error">{error}</span>}
      </div>

      {pipelines && pipelines.length > 0 && (
        <div className="pipeline-list">
          <h3 className="pipeline-list-title">Recent Pipelines</h3>
          {pipelines.map((p: PipelineStatusResponse) => (
            <button
              key={p.id}
              type="button"
              className="pipeline-list-item"
              onClick={() => onStart(p.id)}
            >
              <span className="pipeline-list-id">{p.id.slice(0, 8)}</span>
              <span className={`status-badge ${p.status}`}>{p.status}</span>
            </button>
          ))}
        </div>
      )}
    </form>
  );
}
