import { buildUrl, apiGet, apiPatch, apiPost } from '#renderer/api';
import log from '#renderer/logger';

// ---------------------------------------------------------------------------
// Workbenches
// ---------------------------------------------------------------------------

export interface WorkbenchItem {
  id: number;
  rev: number;
  project: string;
  worktree: string;
  title: string;
  createdAt: string;
  pinnedAt?: string | null;
  archivedAt?: string | null;
  deletedAt?: string | null;
}

export async function fetchWorkbenches(): Promise<WorkbenchItem[]> {
  const res = await apiGet<{ workbenches: WorkbenchItem[] }>('/api/workbenches');
  return res.workbenches;
}

export function archiveWorkbench(id: number): Promise<WorkbenchItem> {
  return apiPatch<WorkbenchItem>(`/api/workbenches/${id}`, { archived: true });
}

export function pinWorkbench(id: number, pinned: boolean): Promise<WorkbenchItem> {
  return apiPatch<WorkbenchItem>(`/api/workbenches/${id}`, { pinned });
}

// ---------------------------------------------------------------------------
// Worktrees
// ---------------------------------------------------------------------------

export interface CreateWorktreeResult {
  ok: boolean;
  path: string;
  branch: string;
  project: string;
}

export function createWorktree(project: string, name?: string): Promise<CreateWorktreeResult> {
  return apiPost<CreateWorktreeResult>('/api/worktrees', { project, name });
}

export interface TerminalSessionInfo {
  id: string;
  rev: number;
  project: string;
  cwd: string;
  agent: 'claude' | 'codex';
  running: boolean;
  exit_code: number | null;
  state?: 'live' | 'restored' | 'exited';
  restored?: boolean;
  createdAt?: string;
  endedAt?: string | null;
  terminalId?: string | null;
  name?: string | null;
  ccSessionId?: string | null;
}

export interface CreateTerminalParams {
  project: string;
  cwd: string;
  agent: 'claude' | 'codex';
  resume_session_id?: string;
  size?: { rows: number; cols: number };
  name?: string;
}

export async function createTerminal(params: CreateTerminalParams): Promise<TerminalSessionInfo> {
  const res = await fetch(buildUrl('/api/terminal'), {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(params),
  });
  if (!res.ok) {
    const err = await res.text();
    throw new Error(`createTerminal failed: ${err}`);
  }
  return res.json();
}

export async function listTerminals(): Promise<TerminalSessionInfo[]> {
  const res = await fetch(buildUrl('/api/terminal'));
  if (!res.ok) throw new Error('listTerminals failed');
  return res.json();
}

export async function getTerminal(id: string): Promise<TerminalSessionInfo> {
  const res = await fetch(buildUrl(`/api/terminal/${id}`));
  if (!res.ok) throw new Error('getTerminal failed');
  return res.json();
}

export async function writeTerminal(id: string, data: string): Promise<void> {
  const encoder = new TextEncoder();
  const bytes = encoder.encode(data);
  const binary = Array.from(bytes)
    .map((b) => String.fromCharCode(b))
    .join('');
  const encoded = btoa(binary);
  const res = await fetch(buildUrl(`/api/terminal/${id}/write`), {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ data: encoded }),
  });
  if (!res.ok) {
    log.warn('terminal write failed', { id, status: res.status });
  }
}

export async function writeTerminalBytes(id: string, bytes: Uint8Array): Promise<void> {
  const binary = Array.from(bytes)
    .map((b) => String.fromCharCode(b))
    .join('');
  const encoded = btoa(binary);
  const res = await fetch(buildUrl(`/api/terminal/${id}/write`), {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ data: encoded }),
  });
  if (!res.ok) {
    log.warn('terminal write failed', { id, status: res.status });
  }
}

export async function resizeTerminal(id: string, rows: number, cols: number): Promise<void> {
  const res = await fetch(buildUrl(`/api/terminal/${id}/resize`), {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ rows, cols }),
  });
  if (!res.ok) {
    log.warn('terminal resize failed', { id, status: res.status });
  }
}

export async function deleteTerminal(id: string): Promise<void> {
  await fetch(buildUrl(`/api/terminal/${id}`), { method: 'DELETE' });
}

export function connectTerminalStream(
  id: string,
  onOutput: (data: Uint8Array) => void,
  onExit: (code: number | null) => void,
  onOpen?: () => void,
  onError?: (err: Event) => void,
  options?: { replay?: boolean },
): EventSource {
  const params = new URLSearchParams();
  if (options?.replay === false) params.set('replay', '0');
  const query = params.size > 0 ? `?${params.toString()}` : '';
  const url = buildUrl(`/api/terminal/${id}/stream${query}`);
  const es = new EventSource(url);

  es.addEventListener('output', (e: MessageEvent) => {
    const raw = atob(e.data);
    const bytes = new Uint8Array(raw.length);
    for (let i = 0; i < raw.length; i++) {
      bytes[i] = raw.charCodeAt(i);
    }
    onOutput(bytes);
  });

  es.addEventListener('exit', (e: MessageEvent) => {
    const data = JSON.parse(e.data);
    onExit(data.code ?? null);
    es.close();
  });

  es.onopen = () => {
    onOpen?.();
  };

  es.onerror = (e) => {
    onError?.(e);
  };

  return es;
}
