import { useState } from "react";
import { useSWRConfig } from "swr";
import type { AuthSession } from "@qltysh/fabro-api-client";

import { ApiError, apiData, authApi } from "../lib/api-client";
import { useAuthSessions } from "../lib/queries";
import { queryKeys } from "../lib/query-keys";
import {
  Badge,
  Mono,
  Muted,
  Panel,
  PanelSkeleton,
} from "../components/settings-panel";
import { formatAbsoluteTs, formatRelativeTime } from "../lib/format";

export default function ProfileSessions() {
  const { data, error } = useAuthSessions();
  const { mutate } = useSWRConfig();
  const [revokingId, setRevokingId] = useState<string | null>(null);
  const [revokeError, setRevokeError] = useState<string | null>(null);

  if (error) {
    return (
      <div className="space-y-6">
        <Panel title="Sessions">
          <div className="px-4 py-6 text-sm text-fg-2">
            Couldn&apos;t load sessions. Please try again.
          </div>
        </Panel>
      </div>
    );
  }

  if (!data) {
    return (
      <div className="space-y-6">
        <PanelSkeleton />
      </div>
    );
  }

  const sessions = sortSessions(data.sessions);

  async function revoke(id: string) {
    setRevokeError(null);
    setRevokingId(id);
    try {
      await apiData(() => authApi.deleteAuthSession(id));
      await mutate(queryKeys.auth.sessions());
    } catch (e) {
      const message =
        e instanceof ApiError && e.message
          ? e.message
          : "Couldn't revoke this session. Please try again.";
      setRevokeError(message);
    } finally {
      setRevokingId(null);
    }
  }

  return (
    <div className="space-y-6">
      <Panel title="Sessions">
        {sessions.length === 0 ? (
          <div className="px-4 py-6 text-sm text-fg-muted">No sessions.</div>
        ) : (
          sessions.map((session) => (
            <SessionRow
              key={session.id}
              session={session}
              onRevoke={revoke}
              pending={revokingId === session.id}
              disabled={revokingId !== null}
            />
          ))
        )}
      </Panel>
      {revokeError ? (
        <div
          role="alert"
          className="text-sm text-rose-400"
          data-testid="revoke-error"
        >
          {revokeError}
        </div>
      ) : null}
    </div>
  );
}

function sortSessions(sessions: AuthSession[]): AuthSession[] {
  return [...sessions].sort((a, b) => {
    if (a.current !== b.current) return a.current ? -1 : 1;
    return Date.parse(b.lastSeenAt) - Date.parse(a.lastSeenAt);
  });
}

function SessionRow({
  session,
  onRevoke,
  pending,
  disabled,
}: {
  session: AuthSession;
  onRevoke: (id: string) => void;
  pending: boolean;
  disabled: boolean;
}) {
  return (
    <div className="grid grid-cols-[minmax(0,1fr)_auto] items-start gap-4 px-4 py-3.5">
      <div className="min-w-0 space-y-1">
        <div className="flex flex-wrap items-center gap-2">
          <span className="text-sm text-fg">{session.label}</span>
          <Badge>{session.kind}</Badge>
          {session.current ? <Badge>current</Badge> : null}
        </div>
        <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-fg-muted">
          <span>
            <Muted>Provider</Muted> <Mono>{session.provider}</Mono>
          </span>
          <span>
            <Muted>Login</Muted> <Mono>{session.login}</Mono>
          </span>
        </div>
        <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-fg-muted">
          <span>
            <Muted>Last active</Muted>{" "}
            <span title={formatAbsoluteTs(session.lastSeenAt)}>
              {formatRelativeTime(session.lastSeenAt)}
            </span>
          </span>
          <span>
            <Muted>Expires</Muted>{" "}
            <span title={formatAbsoluteTs(session.expiresAt)}>
              {formatAbsoluteTs(session.expiresAt)}
            </span>
          </span>
        </div>
        {session.userAgent ? (
          <div className="truncate text-xs text-fg-muted" title={session.userAgent}>
            <Muted>User agent</Muted>{" "}
            <span className="font-mono text-fg-3">{session.userAgent}</span>
          </div>
        ) : null}
      </div>
      <div className="flex shrink-0 items-center">
        {session.revocable ? (
          <button
            type="button"
            onClick={() => onRevoke(session.id)}
            disabled={disabled}
            aria-label={`Revoke ${session.label}`}
            className="rounded-md border border-line bg-overlay px-2.5 py-1 text-xs text-fg-2 transition-colors hover:bg-overlay-strong hover:text-fg disabled:cursor-not-allowed disabled:opacity-50"
          >
            {pending ? "Revoking…" : "Revoke"}
          </button>
        ) : null}
      </div>
    </div>
  );
}
