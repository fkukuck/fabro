import { AuthLayout } from "../components/auth-layout";
import { PRIMARY_BUTTON_CLASS } from "../components/ui";

const steps = [
  {
    title: "Open a terminal on the server host",
    body: (
      <p className="text-sm/6 text-fg-3">
        Run{" "}
        <code className="font-mono text-fg-2">fabro install</code> on the same
        host that runs the Fabro server.
      </p>
    ),
  },
  {
    title: "Choose GitHub App setup",
    body: (
      <p className="text-sm/6 text-fg-3">
        The CLI opens GitHub, exchanges the manifest code, and writes the
        required settings and secrets locally.
      </p>
    ),
  },
  {
    title: "Restart the server, then return to sign in",
    body: (
      <p className="text-sm/6 text-fg-3">
        Once the server comes back up, you can authenticate from the browser.
      </p>
    ),
  },
];

export default function Setup() {
  return (
    <AuthLayout footer="GitHub App setup is managed from the terminal, not the browser.">
      <h1 className="text-center text-2xl font-semibold tracking-tight text-fg text-balance sm:text-[1.75rem]">
        Set up Fabro
      </h1>
      <p className="mt-3 text-center text-sm/6 text-fg-3 text-pretty">
        Run the installer on the server host to register a GitHub App and write
        local configuration.
      </p>
      <ol
        role="list"
        className="mt-8 divide-y divide-line border-y border-line"
      >
        {steps.map((step, index) => (
          <li key={step.title} className="flex items-start gap-4 py-4">
            <span
              className="mt-0.5 flex size-6 shrink-0 items-center justify-center rounded-full bg-overlay text-xs font-semibold tabular-nums text-fg-2 outline-1 -outline-offset-1 outline-white/10"
              aria-hidden="true"
            >
              {index + 1}
            </span>
            <div className="min-w-0">
              <p className="text-sm font-medium text-fg">{step.title}</p>
              <div className="mt-1">{step.body}</div>
            </div>
          </li>
        ))}
      </ol>
      <a href="/login" className={`${PRIMARY_BUTTON_CLASS} mt-8 w-full`}>
        Continue to sign in
      </a>
    </AuthLayout>
  );
}
