import type {
  ClarifyResponse,
  NudgeResponse,
  TaskItem,
  TaskProvider,
} from '#renderer/global/types';
import { z } from 'zod';
import {
  apiGetRouteR,
  apiMultipartRouteR,
  apiPatchRouteR,
  apiPostRouteR,
} from '#renderer/global/providers/http';
import { parseError as makeParseError, type ApiError, type ResultAsync, errAsync } from '#result';

const taskAddMultipartInputSchema = z
  .object({
    title: z.string(),
    project: z.string().optional(),
    provider: z.enum(['claude', 'codex']).optional(),
    useGlmWorker: z.boolean().optional(),
    noAutoMerge: z.boolean().optional(),
    images: z.array(z.instanceof(File)).optional(),
  })
  .strict();

// Tasks
export const fetchTasks = (includeArchived?: boolean) =>
  apiGetRouteR('getTasks', {
    query: includeArchived ? { include_archived: true } : undefined,
  });

export interface AddTaskInput {
  title: string;
  project?: string;
  provider?: Extract<TaskProvider, 'claude' | 'codex'>;
  useGlmWorker?: boolean;
  noAutoMerge?: boolean;
  images?: File[];
}

export function addTask(input: AddTaskInput): ResultAsync<TaskItem, ApiError> {
  const parsedInput = taskAddMultipartInputSchema.safeParse(input);
  if (!parsedInput.success) {
    return errAsync(makeParseError(parsedInput.error.issues, 'route:postTasksAdd multipart'));
  }

  const data = parsedInput.data;
  const form = new FormData();
  form.append('title', data.title);
  form.append('source', 'electron');
  if (data.project) form.append('project', data.project);
  if (data.provider) form.append('provider', data.provider);
  if (typeof data.useGlmWorker === 'boolean') {
    form.append('use_glm_worker', data.useGlmWorker ? 'true' : 'false');
  }
  if (data.noAutoMerge) form.append('no_auto_merge', 'true');
  if (data.images) {
    for (const img of data.images) {
      form.append('images', img, img.name);
    }
  }
  return apiMultipartRouteR('postTasksAdd', form, undefined, {
    title: data.title,
    project: data.project ?? null,
    provider: data.provider ?? null,
    use_glm_worker: data.useGlmWorker ?? false,
    plan: false,
    no_pr: false,
  });
}

export const deleteItems = (ids: number[], opts?: { close_pr?: boolean; force?: boolean }) =>
  apiPostRouteR('postTasksDelete', { ids, ...opts });

export const acceptItem = (id: number) => apiPostRouteR('postTasksAccept', { id });

export function reopenItem(id: number, feedback: string, images?: File[]) {
  if (images?.length) {
    const form = new FormData();
    form.append('id', String(id));
    form.append('feedback', feedback);
    for (const img of images) form.append('images', img, img.name);
    return apiMultipartRouteR('postTasksReopen', form, undefined, { id, feedback });
  }
  return apiMultipartRouteR('postTasksReopen', { id, feedback });
}

export function reworkItem(id: number, feedback: string, images?: File[]) {
  if (images?.length) {
    const form = new FormData();
    form.append('id', String(id));
    form.append('feedback', feedback);
    for (const img of images) form.append('images', img, img.name);
    return apiMultipartRouteR('postTasksRework', form, undefined, { id, feedback });
  }
  return apiMultipartRouteR('postTasksRework', { id, feedback });
}

export const fetchTimeline = (id: number) =>
  apiGetRouteR('getTasksByIdTimeline', { params: { id } });
export const fetchItemSessions = (id: number) =>
  apiGetRouteR('getTasksByIdSessions', { params: { id } });

// Manual override for the clarifier's bug-fix classification. When the user
// flips the toggle in the task editor we PATCH only the `is_bug_fix` field;
// the captain workflow picks it up on the next worker spawn / review tick.
export const setTaskIsBugFix = (id: number, value: boolean) =>
  apiPatchRouteR('patchTasksById', { is_bug_fix: value }, { params: { id } });

// Retry / Resume / Clarify
export const retryItem = (id: number) => apiPostRouteR('postTasksRetry', { id });
export const resumeRateLimited = (id: number) =>
  apiPostRouteR('postTasksResumeratelimited', { id });

// `wait: false` makes the clarify endpoint ack as soon as the answer
// is committed and spawn the follow-up CC reclarify call on the daemon
// task tracker. The renderer relies on SSE for the next state, so the
// clarification form unblocks immediately rather than waiting on the
// long CC roundtrip.
function clarifyOptions(id: number) {
  return { params: { id }, query: { wait: false } };
}

export function answerClarification(
  id: number,
  answers: { question: string; answer: string }[],
  images?: File[],
): ResultAsync<ClarifyResponse, ApiError> {
  if (images?.length) {
    const form = new FormData();
    form.append('answers', JSON.stringify(answers));
    for (const img of images) form.append('images', img, img.name);
    return apiMultipartRouteR('postTasksByIdClarify', form, clarifyOptions(id), { answers });
  }
  return apiMultipartRouteR('postTasksByIdClarify', { answers }, clarifyOptions(id));
}

/** Flat-text answer for Telegram-style input */
export function answerClarificationText(
  id: number,
  answer: string,
  images?: File[],
): ResultAsync<ClarifyResponse, ApiError> {
  if (images?.length) {
    const form = new FormData();
    form.append('answer', answer);
    for (const img of images) form.append('images', img, img.name);
    return apiMultipartRouteR('postTasksByIdClarify', form, clarifyOptions(id), { answer });
  }
  return apiMultipartRouteR('postTasksByIdClarify', { answer }, clarifyOptions(id));
}

// Captain
export const triggerTick = (dryRun = false) =>
  apiPostRouteR('postCaptainTick', { dry_run: dryRun, emit_notifications: true });

export function nudgeWorker(
  itemId: number,
  message: string,
  images?: File[],
): ResultAsync<NudgeResponse, ApiError> {
  if (images?.length) {
    const form = new FormData();
    form.append('item_id', String(itemId));
    form.append('message', message);
    for (const img of images) form.append('images', img, img.name);
    return apiMultipartRouteR('postCaptainNudge', form, undefined, {
      item_id: String(itemId),
      message,
    });
  }
  return apiMultipartRouteR('postCaptainNudge', {
    item_id: String(itemId),
    message,
  });
}

export const handoffItem = (id: number) => apiPostRouteR('postTasksHandoff', { id });
export const stopItem = (id: number) => apiPostRouteR('postTasksStop', { id });
export const cancelItem = (id: number) => apiPostRouteR('postTasksCancel', { id });

// Workers
export const fetchWorkers = () => apiGetRouteR('getWorkers');

// Stats
export const fetchActivityStats = () => apiGetRouteR('getStatsActivity');

// Task Artifacts
export const fetchArtifacts = (id: number) =>
  apiGetRouteR('getTasksByIdArtifacts', { params: { id } });
// Task Feed (unified timeline + artifacts)
export const fetchFeed = (id: number) => apiGetRouteR('getTasksByIdFeed', { params: { id } });

// Merge PR
export const mergePr = (prNumber: number, project: string) =>
  apiPostRouteR('postTasksMerge', { pr_number: prNumber, project });

// PR Summary
export const fetchPrSummary = (id: number) =>
  apiGetRouteR('getTasksByIdPrsummary', { params: { id } });
