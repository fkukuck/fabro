const stages = [
  { stage: "Detect Drift", model: "Opus 4.6", inputTokens: 12_480, outputTokens: 3_210, runtime: "1m 12s", cost: 0.48 },
  { stage: "Propose Changes", model: "Gemini 3.1", inputTokens: 28_640, outputTokens: 8_750, runtime: "2m 34s", cost: 0.72 },
  { stage: "Review Changes", model: "Codex 5.3", inputTokens: 9_120, outputTokens: 2_640, runtime: "0m 45s", cost: 0.19 },
  { stage: "Apply Changes", model: "Opus 4.6", inputTokens: 21_300, outputTokens: 6_480, runtime: "1m 58s", cost: 0.87 },
];

const totalRuntime = "6m 29s";
const totalCost = stages.reduce((sum, s) => sum + s.cost, 0);
const totalInput = stages.reduce((sum, s) => sum + s.inputTokens, 0);
const totalOutput = stages.reduce((sum, s) => sum + s.outputTokens, 0);

const modelBreakdown = Object.values(
  stages.reduce<Record<string, { model: string; inputTokens: number; outputTokens: number; cost: number; stages: number }>>(
    (acc, s) => {
      const entry = acc[s.model] ?? { model: s.model, inputTokens: 0, outputTokens: 0, cost: 0, stages: 0 };
      entry.inputTokens += s.inputTokens;
      entry.outputTokens += s.outputTokens;
      entry.cost += s.cost;
      entry.stages += 1;
      acc[s.model] = entry;
      return acc;
    },
    {},
  ),
).sort((a, b) => b.cost - a.cost);

function formatTokens(n: number) {
  return `${(n / 1000).toFixed(1)}k`;
}

export default function RunUsage() {
  return (
    <div className="space-y-6">
      <div className="rounded-md border border-line overflow-hidden">
        <table className="w-full text-sm">
          <thead>
            <tr className="border-b border-line bg-panel/60 text-left text-xs text-fg-muted">
              <th className="px-4 py-2.5 font-medium">Stage</th>
              <th className="px-4 py-2.5 font-medium">Model</th>
              <th className="px-4 py-2.5 font-medium text-right">Tokens</th>
              <th className="px-4 py-2.5 font-medium text-right">Run time</th>
              <th className="px-4 py-2.5 font-medium text-right">Cost</th>
            </tr>
          </thead>
          <tbody>
            {stages.map((row) => (
              <tr key={row.stage} className="border-b border-line last:border-b-0">
                <td className="px-4 py-3 text-fg-2">{row.stage}</td>
                <td className="px-4 py-3 font-mono text-xs text-fg-3">{row.model}</td>
                <td className="px-4 py-3 text-right font-mono text-xs tabular-nums text-fg-3">
                  {formatTokens(row.inputTokens)} <span className="text-fg-muted">/</span> {formatTokens(row.outputTokens)}
                </td>
                <td className="px-4 py-3 text-right font-mono text-xs text-fg-3">{row.runtime}</td>
                <td className="px-4 py-3 text-right font-mono text-xs text-fg-3">${row.cost.toFixed(2)}</td>
              </tr>
            ))}
          </tbody>
          <tfoot>
            <tr className="border-t border-line-strong bg-panel/40">
              <td className="px-4 py-3 font-medium text-fg">Total</td>
              <td />
              <td className="px-4 py-3 text-right font-mono text-xs tabular-nums font-medium text-fg">
                {formatTokens(totalInput)} <span className="text-fg-muted">/</span> {formatTokens(totalOutput)}
              </td>
              <td className="px-4 py-3 text-right font-mono text-xs font-medium text-fg">{totalRuntime}</td>
              <td className="px-4 py-3 text-right font-mono text-xs font-medium text-fg">${totalCost.toFixed(2)}</td>
            </tr>
          </tfoot>
        </table>
      </div>

      <div>
        <h3 className="mb-3 text-xs font-medium uppercase tracking-wider text-fg-muted">By Model</h3>
        <div className="rounded-md border border-line overflow-hidden">
          <table className="w-full text-sm">
            <thead>
              <tr className="border-b border-line bg-panel/60 text-left text-xs text-fg-muted">
                <th className="px-4 py-2.5 font-medium">Model</th>
                <th className="px-4 py-2.5 font-medium text-right">Stages</th>
                <th className="px-4 py-2.5 font-medium text-right">Tokens</th>
                <th className="px-4 py-2.5 font-medium text-right">Cost</th>
              </tr>
            </thead>
            <tbody>
              {modelBreakdown.map((row) => (
                <tr key={row.model} className="border-b border-line last:border-b-0">
                  <td className="px-4 py-3 font-mono text-xs text-fg-2">{row.model}</td>
                  <td className="px-4 py-3 text-right font-mono text-xs tabular-nums text-fg-3">{row.stages}</td>
                  <td className="px-4 py-3 text-right font-mono text-xs tabular-nums text-fg-3">
                    {formatTokens(row.inputTokens)} <span className="text-fg-muted">/</span> {formatTokens(row.outputTokens)}
                  </td>
                  <td className="px-4 py-3 text-right font-mono text-xs text-fg-3">${row.cost.toFixed(2)}</td>
                </tr>
              ))}
            </tbody>
            <tfoot>
              <tr className="border-t border-line-strong bg-panel/40">
                <td className="px-4 py-3 font-medium text-fg">Total</td>
                <td className="px-4 py-3 text-right font-mono text-xs tabular-nums font-medium text-fg">{stages.length}</td>
                <td className="px-4 py-3 text-right font-mono text-xs tabular-nums font-medium text-fg">
                  {formatTokens(totalInput)} <span className="text-fg-muted">/</span> {formatTokens(totalOutput)}
                </td>
                <td className="px-4 py-3 text-right font-mono text-xs font-medium text-fg">${totalCost.toFixed(2)}</td>
              </tr>
            </tfoot>
          </table>
        </div>
      </div>
    </div>
  );
}
