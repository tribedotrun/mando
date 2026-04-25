import {
  apiDeleteRouteR,
  apiGetRouteR,
  apiPatchRouteR,
  apiPostRouteR,
  openSseRoute,
} from '#renderer/global/providers/http';
import log from '#renderer/global/service/logger';
import type { RouteEvent } from '#shared/daemon-contract/runtime';
import { eventSchemas } from '#shared/daemon-contract/schemas';
import type {
  CreateWorktreeResponse,
  WorkbenchItem,
  WorkbenchStatusFilter,
  TerminalSessionInfo,
} from '#renderer/global/types';
import { type ApiError, type ResultAsync, ApiErrorThrown, parseSseMessage } from '#result';
import { decodeTerminalOutputPayload } from '#renderer/domains/captain/repo/terminalOutput.ts';

export type {
  WorkbenchItem,
  WorkbenchStatusFilter,
  TerminalState,
  TerminalSessionInfo,
} from '#renderer/global/types';

// ---------------------------------------------------------------------------
// Workbenches
// ---------------------------------------------------------------------------

export function fetchWorkbenches(
  status?: WorkbenchStatusFilter,
): ResultAsync<WorkbenchItem[], ApiError> {
  return apiGetRouteR('getWorkbenches', {
    query: status && status !== 'active' ? { status } : undefined,
  }).map((res) => res.workbenches);
}

export function archiveWorkbench(id: number): ResultAsync<WorkbenchItem, ApiError> {
  return apiPatchRouteR('patchWorkbenchesById', { archived: true }, { params: { id } });
}

export function unarchiveWorkbench(id: number): ResultAsync<WorkbenchItem, ApiError> {
  return apiPatchRouteR('patchWorkbenchesById', { archived: false }, { params: { id } });
}

export function pinWorkbench(id: number, pinned: boolean): ResultAsync<WorkbenchItem, ApiError> {
  return apiPatchRouteR('patchWorkbenchesById', { pinned }, { params: { id } });
}

export function renameWorkbench(id: number, title: string): ResultAsync<WorkbenchItem, ApiError> {
  return apiPatchRouteR('patchWorkbenchesById', { title }, { params: { id } });
}

// ---------------------------------------------------------------------------
// Worktrees
// ---------------------------------------------------------------------------

export function createWorktree(
  project: string,
  name?: string,
): ResultAsync<CreateWorktreeResponse, ApiError> {
  return apiPostRouteR('postWorktrees', { project, name });
}

export interface CreateTerminalParams {
  project: string;
  cwd: string;
  agent: 'claude' | 'codex';
  resume_session_id?: string;
  size?: { rows: number; cols: number };
  name?: string;
}

export function createTerminal(
  params: CreateTerminalParams,
): ResultAsync<TerminalSessionInfo, ApiError> {
  return apiPostRouteR('postTerminal', params);
}

export function listTerminals(): ResultAsync<TerminalSessionInfo[], ApiError> {
  return apiGetRouteR('getTerminal');
}

export function getTerminal(id: string): ResultAsync<TerminalSessionInfo, ApiError> {
  return apiGetRouteR('getTerminalById', { params: { id } });
}

export async function writeTerminalBytes(id: string, bytes: Uint8Array): Promise<void> {
  const binary = Array.from(bytes)
    .map((b) => String.fromCharCode(b))
    .join('');
  const encoded = btoa(binary);
  const result = await apiPostRouteR(
    'postTerminalByIdWrite',
    { data: encoded },
    { params: { id } },
  );
  // Fire-and-forget: callers in terminalRuntime log best-effort write failures.
  // No mutation hook depends on rejection, so log-only is intentional.
  result.match(
    () => undefined,
    (error) => log.warn('terminal write failed', { id, error }),
  );
}

export async function resizeTerminal(id: string, rows: number, cols: number): Promise<void> {
  const result = await apiPostRouteR('postTerminalByIdResize', { rows, cols }, { params: { id } });
  // Fire-and-forget: callers in terminalRuntime log best-effort resize failures.
  // No mutation hook depends on rejection, so log-only is intentional.
  result.match(
    () => undefined,
    (error) => log.warn('terminal resize failed', { id, error }),
  );
}

export async function deleteTerminal(id: string): Promise<void> {
  const result = await apiDeleteRouteR('deleteTerminalById', { params: { id } });
  // useTerminalDelete relies on rejection to trigger onError rollback + toast.
  if (result.isErr()) {
    // invariant: mutationFn boundary -- ApiErrorThrown propagates to React Query onError
    throw new ApiErrorThrown(result.error);
  }
}

export function connectTerminalStream(
  id: string,
  onOutput: (data: Uint8Array) => void,
  onExit: (code: number | null) => void,
  onOpen?: () => void,
  onError?: (err: Event) => void,
  options?: { replay?: boolean },
): EventSource {
  const es = openSseRoute('getTerminalByIdStream', {
    params: { id },
    query: options?.replay === false ? { replay: 0 } : undefined,
  });

  es.onmessage = (e) => {
    const parsed = parseSseMessage(e.data as unknown, eventSchemas.getTerminalByIdStream);
    if (parsed.failure) {
      log.error('terminal stream event parse failed', parsed.failure);
      onExit(null);
      es.close();
      return;
    }
    const envelope = parsed.data as RouteEvent<'getTerminalByIdStream'>;

    switch (envelope.event) {
      case 'output': {
        const decoded = decodeTerminalOutputPayload(envelope.data);
        if (decoded.isErr()) {
          log.error('terminal stream output parse failed', decoded.error);
          onExit(null);
          es.close();
          return;
        }
        onOutput(decoded.value);
        return;
      }

      case 'exit':
        onExit(envelope.data.code ?? null);
        es.close();
        return;

      default: {
        const unexpected: never = envelope;
        log.error('unexpected terminal stream event', unexpected);
        onExit(null);
        es.close();
      }
    }
  };

  es.onopen = () => {
    onOpen?.();
  };

  es.onerror = (e) => {
    onError?.(e);
  };

  return es;
}
