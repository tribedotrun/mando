import type { AskResponse, ClarifyResponse, NudgeResponse, TaskItem } from '#renderer/global/types';
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
    noAutoMerge: z.boolean().optional(),
    planning: z.boolean().optional(),
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
  noAutoMerge?: boolean;
  planning?: boolean;
  images?: File[];
}

export const parseTodos = (text: string, project: string) =>
  apiPostRouteR('postAiParsetodos', { text, project });

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
  if (data.noAutoMerge) form.append('no_auto_merge', 'true');
  if (data.planning) form.append('planning', 'true');
  if (data.images) {
    for (const img of data.images) {
      form.append('images', img, img.name);
    }
  }
  return apiMultipartRouteR('postTasksAdd', form, undefined, {
    title: data.title,
    project: data.project ?? null,
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

/** Start implementation: inject plan into context and re-queue as normal worker. */
export function startImplementation(
  id: number,
  existingContext: string,
): ResultAsync<void, ApiError> {
  return fetchTimeline(id).andThen((tl) => {
    const completedPlan = [...tl.events]
      .reverse()
      .find((event) => event.data.event_type === 'plan_completed');
    const plan = completedPlan?.data.event_type === 'plan_completed' ? completedPlan.data.plan : '';
    const body = plan
      ? `${existingContext}\n\n## Approved Plan\n${plan}\n\n[Human] Start implementation.`
      : `${existingContext}\n\n[Human] Start implementation.`;
    return apiPatchRouteR('patchTasksById', { context: body }, { params: { id } })
      .andThen(() => apiPostRouteR('postTasksQueue', { id }))
      .map(() => undefined);
  });
}

// Retry / Resume / Clarify
export const retryItem = (id: number) => apiPostRouteR('postTasksRetry', { id });
export const resumeRateLimited = (id: number) =>
  apiPostRouteR('postTasksResumeratelimited', { id });

export function answerClarification(
  id: number,
  answers: { question: string; answer: string }[],
  images?: File[],
): ResultAsync<ClarifyResponse, ApiError> {
  if (images?.length) {
    const form = new FormData();
    form.append('answers', JSON.stringify(answers));
    for (const img of images) form.append('images', img, img.name);
    return apiMultipartRouteR('postTasksByIdClarify', form, { params: { id } }, { answers });
  }
  return apiMultipartRouteR('postTasksByIdClarify', { answers }, { params: { id } });
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
    return apiMultipartRouteR('postTasksByIdClarify', form, { params: { id } }, { answer });
  }
  return apiMultipartRouteR('postTasksByIdClarify', { answer }, { params: { id } });
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

// Task Ask (multi-turn: first ask creates session, follow-ups resume)
export function askTask(
  id: number,
  question: string,
  askId?: string,
  images?: File[],
): ResultAsync<AskResponse, ApiError> {
  if (images?.length) {
    const form = new FormData();
    form.append('id', String(id));
    form.append('question', question);
    if (askId) form.append('ask_id', askId);
    for (const img of images) form.append('images', img, img.name);
    return apiMultipartRouteR('postTasksAsk', form, undefined, {
      id,
      question,
      ask_id: askId,
    });
  }
  return apiMultipartRouteR('postTasksAsk', { id, question, ask_id: askId });
}

// End ask session
export const endAskSession = (id: number) => apiPostRouteR('postTasksAskEnd', { id });
// Reopen from Q&A — synthesize conversation into reopen feedback
export const askReopen = (id: number) => apiPostRouteR('postTasksAskReopen', { id });

// Task Ask History
export const fetchAskHistory = (id: number) =>
  apiGetRouteR('getTasksByIdHistory', { params: { id } });
// Task Artifacts
export const fetchArtifacts = (id: number) =>
  apiGetRouteR('getTasksByIdArtifacts', { params: { id } });
// Task Feed (unified timeline + artifacts + messages)
export const fetchFeed = (id: number) => apiGetRouteR('getTasksByIdFeed', { params: { id } });

// Task Advisor
export const sendAdvisorMessage = (id: number, message: string, intent: string = 'ask') =>
  apiPostRouteR('postTasksByIdAdvisor', { message, intent }, { params: { id } });

// Merge PR
export const mergePr = (prNumber: number, project: string) =>
  apiPostRouteR('postTasksMerge', { pr_number: prNumber, project });

// PR Summary
export const fetchPrSummary = (id: number) =>
  apiGetRouteR('getTasksByIdPrsummary', { params: { id } });

// Used by error-mapping helpers; surface as a sibling export to keep the import surface tidy.
export { errAsync as _errAsync };
