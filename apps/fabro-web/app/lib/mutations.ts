import useSWRMutation from "swr/mutation";
import { useSWRConfig } from "swr";
import type {
  ErrorResponseEntry,
  PreviewUrlResponse,
  RunStatusResponse,
} from "@qltysh/fabro-api-client";

import { apiJsonMutation, apiRequest } from "./api-client";
import { queryKeys } from "./query-keys";
import type { LifecycleAction, LifecycleActionError } from "./run-actions";
import {
  archiveRun,
  cancelRun,
  unarchiveRun,
} from "./run-actions";

export type PreviewRunArg = {
  port: number;
  expires_in_secs: number;
};

export type PreviewMutationResult = {
  intent: "preview";
  url: string;
};

export type LifecycleMutationResult =
  | {
      intent: LifecycleAction;
      ok: true;
      run: RunStatusResponse;
    }
  | {
      intent: LifecycleAction;
      ok: false;
      error: LifecycleActionError | null;
    };

export function usePreviewRun(id: string | undefined) {
  return useSWRMutation(
    id ? queryKeys.runs.preview(id) : null,
    async (key: string, { arg }: { arg: PreviewRunArg }): Promise<PreviewMutationResult> => {
      const result = await apiJsonMutation<PreviewUrlResponse, PreviewRunArg>(key, { arg });
      return { intent: "preview", url: result.url };
    },
  );
}

export function useCancelRun(id: string | undefined) {
  return useLifecycleMutation(id, "cancel", cancelRun);
}

export function useArchiveRun(id: string | undefined) {
  return useLifecycleMutation(id, "archive", archiveRun);
}

export function useUnarchiveRun(id: string | undefined) {
  return useLifecycleMutation(id, "unarchive", unarchiveRun);
}

function useLifecycleMutation(
  id: string | undefined,
  intent: LifecycleAction,
  action: (id: string) => Promise<RunStatusResponse>,
) {
  const { mutate } = useSWRConfig();
  const key = id ? queryKeys.runs[intent](id) : null;
  return useSWRMutation(
    key,
    async (): Promise<LifecycleMutationResult> => {
      if (!id) {
        return { intent, ok: false, error: null };
      }
      try {
        return { intent, ok: true, run: await action(id) };
      } catch (error) {
        return {
          intent,
          ok: false,
          error: serializeLifecycleActionError(error),
        };
      }
    },
    {
      onSuccess: (result) => {
        if (!id || !result.ok) return;
        void mutate(queryKeys.runs.detail(id));
        void mutate(queryKeys.boards.runs());
        void mutate(queryKeys.runs.billing(id));
      },
    },
  );
}

export function useToggleDemoMode() {
  const { mutate } = useSWRConfig();
  return useSWRMutation(
    queryKeys.demo.toggle(),
    async (key: string, { arg }: { arg: { enabled: boolean } }) => {
      const response = await apiRequest(key, {
        init: {
          method: "POST",
          credentials: "include",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify(arg),
        },
      });
      if (!response.ok) {
        throw new Error(response.statusText || `HTTP ${response.status}`);
      }
      return response;
    },
    {
      onSuccess: () => {
        void mutate(queryKeys.auth.me());
      },
    },
  );
}

export function useLoginDevToken() {
  return useSWRMutation(
    "/auth/login/dev-token",
    async (key: string, { arg }: { arg: { token: string } }) => {
      const response = await fetch(key, {
        method: "POST",
        credentials: "include",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(arg),
      });
      if (!response.ok) {
        throw new Error(response.statusText || `HTTP ${response.status}`);
      }
      return response.json() as Promise<{ ok: boolean }>;
    },
  );
}

function serializeLifecycleActionError(error: unknown): LifecycleActionError | null {
  if (!error || typeof error !== "object") return null;
  const record = error as Record<string, unknown>;
  if (typeof record.status !== "number" || !Array.isArray(record.errors)) {
    return null;
  }

  return {
    status: record.status,
    errors: record.errors.filter(isErrorResponseEntry),
  };
}

function isErrorResponseEntry(value: unknown): value is ErrorResponseEntry {
  if (!value || typeof value !== "object") return false;
  const record = value as Record<string, unknown>;
  return (
    typeof record.status === "string" &&
    typeof record.title === "string" &&
    typeof record.detail === "string"
  );
}
