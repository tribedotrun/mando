import type {
  TaskListResponse,
  TaskItem,
  WorkersResponse,
  TimelineResponse,
  TickResult,
  PrSummaryResponse,
  AskResponse,
  AskHistoryResponse,
  ItemSessionsResponse,
  ArtifactsResponse,
  FeedResponse,
  AdvisorResponse,
  ActivityStatsResponse,
} from '#renderer/global/types';
import { apiGet, apiPost, apiPatch, buildUrl } from '#renderer/global/providers/http';

// Tasks
export const fetchTasks = (includeArchived?: boolean) => {
  const qs = includeArchived ? '?include_archived=true' : '';
  return apiGet<TaskListResponse>(`/api/tasks${qs}`);
};
export interface AddTaskInput {
  title: string;
  project?: string;
  noAutoMerge?: boolean;
  images?: File[];
}

export const parseTodos = (text: string, project: string) =>
  apiPost<{ items: string[] }>('/api/ai/parse-todos', { text, project });

export async function addTask(input: AddTaskInput): Promise<TaskItem> {
  const form = new FormData();
  form.append('title', input.title);
  form.append('source', 'electron');
  if (input.project) form.append('project', input.project);
  if (input.noAutoMerge) form.append('no_auto_merge', 'true');
  if (input.images) {
    for (const img of input.images) {
      form.append('images', img, img.name);
    }
  }
  const res = await fetch(buildUrl('/api/tasks/add'), {
    method: 'POST',
    body: form,
  });
  if (!res.ok) {
    const errBody = await res.json().catch(() => ({ error: res.statusText }));
    throw new Error(errBody.error || `HTTP ${res.status}`);
  }
  return res.json() as Promise<TaskItem>;
}
export const deleteItems = (ids: number[], opts?: { close_pr?: boolean; force?: boolean }) =>
  apiPost<{ ok: boolean; deleted: number; warnings?: string[] }>('/api/tasks/delete', {
    ids,
    ...opts,
  });
export const acceptItem = (id: number) => apiPost<void>('/api/tasks/accept', { id });
export async function reopenItem(id: number, feedback: string, images?: File[]): Promise<void> {
  if (images?.length) {
    const form = new FormData();
    form.append('id', String(id));
    form.append('feedback', feedback);
    for (const img of images) form.append('images', img, img.name);
    const res = await fetch(buildUrl('/api/tasks/reopen'), { method: 'POST', body: form });
    if (!res.ok) {
      const err = await res.json().catch(() => ({ error: res.statusText }));
      throw new Error(err.error || `HTTP ${res.status}`);
    }
    return;
  }
  return apiPost<void>('/api/tasks/reopen', { id, feedback });
}

export async function reworkItem(id: number, feedback: string, images?: File[]): Promise<void> {
  if (images?.length) {
    const form = new FormData();
    form.append('id', String(id));
    form.append('feedback', feedback);
    for (const img of images) form.append('images', img, img.name);
    const res = await fetch(buildUrl('/api/tasks/rework'), { method: 'POST', body: form });
    if (!res.ok) {
      const err = await res.json().catch(() => ({ error: res.statusText }));
      throw new Error(err.error || `HTTP ${res.status}`);
    }
    return;
  }
  return apiPost<void>('/api/tasks/rework', { id, feedback });
}
export const fetchTimeline = (id: number) => apiGet<TimelineResponse>(`/api/tasks/${id}/timeline`);
export const fetchItemSessions = (id: number) =>
  apiGet<ItemSessionsResponse>(`/api/tasks/${id}/sessions`);
/** Start implementation: inject plan into context and re-queue as normal worker. */
export async function startImplementation(id: number, existingContext: string): Promise<void> {
  const tl = await fetchTimeline(id);
  const plan =
    ([...tl.events].reverse().find((e) => e.event_type === 'plan_completed')?.data
      ?.plan as string) ?? '';
  const body = plan
    ? `${existingContext}\n\n## Approved Plan\n${plan}\n\n[Human] Start implementation.`
    : `${existingContext}\n\n[Human] Start implementation.`;
  await apiPatch(`/api/tasks/${id}`, { planning: false, context: body, status: 'queued' });
}
// Retry / Resume / Clarify
export const retryItem = (id: number) => apiPost<{ ok: boolean }>('/api/tasks/retry', { id });
export const resumeRateLimited = (id: number) =>
  apiPost<{ ok: boolean }>('/api/tasks/resume-rate-limited', { id });

export interface ClarifyResponse {
  ok: boolean;
  status: string;
  context?: string;
  questions?: {
    question: string;
    answer?: string | null;
    self_answered: boolean;
    category?: 'code' | 'intent';
  }[];
  session_id?: string;
  error?: string;
}

export async function answerClarification(
  id: number,
  answers: { question: string; answer: string }[],
  images?: File[],
): Promise<ClarifyResponse> {
  if (images?.length) {
    const form = new FormData();
    form.append('answers_json', JSON.stringify(answers));
    for (const img of images) form.append('images', img, img.name);
    const res = await fetch(buildUrl(`/api/tasks/${id}/clarify`), { method: 'POST', body: form });
    if (!res.ok) {
      const err = await res.json().catch(() => ({ error: res.statusText }));
      throw new Error(err.error || `HTTP ${res.status}`);
    }
    return res.json() as Promise<ClarifyResponse>;
  }
  return apiPost<ClarifyResponse>(`/api/tasks/${id}/clarify`, { answers });
}

/** Flat-text answer for Telegram-style input */
export async function answerClarificationText(
  id: number,
  answer: string,
  images?: File[],
): Promise<ClarifyResponse> {
  if (images?.length) {
    const form = new FormData();
    form.append('answer', answer);
    for (const img of images) form.append('images', img, img.name);
    const res = await fetch(buildUrl(`/api/tasks/${id}/clarify`), { method: 'POST', body: form });
    if (!res.ok) {
      const err = await res.json().catch(() => ({ error: res.statusText }));
      throw new Error(err.error || `HTTP ${res.status}`);
    }
    return res.json() as Promise<ClarifyResponse>;
  }
  return apiPost<ClarifyResponse>(`/api/tasks/${id}/clarify`, { answer });
}

// Captain
export const triggerTick = (dryRun = false) =>
  apiPost<TickResult>('/api/captain/tick', { dry_run: dryRun });
export async function nudgeWorker(
  itemId: number,
  message: string,
  images?: File[],
): Promise<{ worker?: string; pid?: number }> {
  if (images?.length) {
    const form = new FormData();
    form.append('item_id', String(itemId));
    form.append('message', message);
    for (const img of images) form.append('images', img, img.name);
    const res = await fetch(buildUrl('/api/captain/nudge'), { method: 'POST', body: form });
    if (!res.ok) {
      const err = await res.json().catch(() => ({ error: res.statusText }));
      throw new Error(err.error || `HTTP ${res.status}`);
    }
    return res.json() as Promise<{ worker?: string; pid?: number }>;
  }
  return apiPost<{ worker?: string; pid?: number }>('/api/captain/nudge', {
    item_id: String(itemId),
    message,
  });
}
export const handoffItem = (id: number) => apiPost<{ ok: boolean }>('/api/tasks/handoff', { id });
export const cancelItem = (id: number) => apiPost<{ ok: boolean }>('/api/tasks/cancel', { id });

// Workers
export const fetchWorkers = () => apiGet<WorkersResponse>('/api/workers');

// Stats
export const fetchActivityStats = () => apiGet<ActivityStatsResponse>('/api/stats/activity');

// Task Ask (multi-turn: first ask creates session, follow-ups resume)
export async function askTask(
  id: number,
  question: string,
  askId?: string,
  images?: File[],
): Promise<AskResponse> {
  if (images?.length) {
    const form = new FormData();
    form.append('id', String(id));
    form.append('question', question);
    if (askId) form.append('ask_id', askId);
    for (const img of images) form.append('images', img, img.name);
    const res = await fetch(buildUrl('/api/tasks/ask'), { method: 'POST', body: form });
    if (!res.ok) {
      const err = await res.json().catch(() => ({ error: res.statusText }));
      throw new Error(err.error || `HTTP ${res.status}`);
    }
    return res.json() as Promise<AskResponse>;
  }
  return apiPost<AskResponse>('/api/tasks/ask', { id, question, ask_id: askId });
}

// End ask session
export const endAskSession = (id: number) =>
  apiPost<{ ok: boolean; ended: string }>('/api/tasks/ask/end', { id });
// Reopen from Q&A — synthesize conversation into reopen feedback
export const askReopen = (id: number) =>
  apiPost<{ ok: boolean; feedback: string }>('/api/tasks/ask/reopen', { id });

// Task Ask History
export const fetchAskHistory = (id: number) =>
  apiGet<AskHistoryResponse>(`/api/tasks/${id}/history`);
// Task Artifacts
export const fetchArtifacts = (id: number) =>
  apiGet<ArtifactsResponse>(`/api/tasks/${id}/artifacts`);
// Task Feed (unified timeline + artifacts + messages)
export const fetchFeed = (id: number) => apiGet<FeedResponse>(`/api/tasks/${id}/feed`);

// Task Advisor
export const sendAdvisorMessage = (id: number, message: string, intent: string = 'ask') =>
  apiPost<AdvisorResponse>(`/api/tasks/${id}/advisor`, { message, intent });

// Merge PR
export const mergePr = (prNumber: number, project: string) =>
  apiPost<{ ok: boolean; message: string }>('/api/tasks/merge', { pr_number: prNumber, project });

// PR Summary
export const fetchPrSummary = (id: number) =>
  apiGet<PrSummaryResponse>(`/api/tasks/${id}/pr-summary`);
