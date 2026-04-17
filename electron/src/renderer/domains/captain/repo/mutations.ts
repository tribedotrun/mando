import { useMutation, useQueryClient } from '@tanstack/react-query';
import { toast } from 'sonner';
import log from '#renderer/global/service/logger';
import {
  addTask,
  parseTodos,
  acceptItem,
  cancelItem,
  retryItem,
  resumeRateLimited,
  handoffItem,
  reopenItem,
  reworkItem,
  askReopen,
  mergePr,
  triggerTick,
  askTask,
  nudgeWorker,
  deleteItems,
  answerClarification,
  answerClarificationText,
  sendAdvisorMessage,
  startImplementation,
  type AddTaskInput,
} from '#renderer/domains/captain/repo/api';
import type { TaskListResponse } from '#renderer/global/types';
import { queryKeys } from '#renderer/global/repo/queryKeys';
import { invalidateTaskDetail } from '#renderer/global/repo/sseCacheHelpers';
import {
  updateTaskInList,
  removeTasksFromList,
} from '#renderer/domains/captain/repo/taskListHelpers';

// ---------------------------------------------------------------------------
// 1. useTaskCreate
// ---------------------------------------------------------------------------

export function useTaskCreate() {
  return useMutation({
    mutationFn: async (input: AddTaskInput) => addTask(input),
    onSuccess: () => {
      toast.success('Task created');
    },
    onError: (err: Error) => {
      toast.error(err.message || 'Failed to create task');
    },
    // SSE handles cache update
  });
}

// ---------------------------------------------------------------------------
// 2. useTaskAccept
// ---------------------------------------------------------------------------

export function useTaskAccept() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (vars: { id: number }) => acceptItem(vars.id),
    onMutate: async (vars) => {
      await qc.cancelQueries({ queryKey: queryKeys.tasks.list() });
      const prev = qc.getQueryData<TaskListResponse>(queryKeys.tasks.list());
      qc.setQueryData<TaskListResponse>(queryKeys.tasks.list(), (old) =>
        updateTaskInList(old, vars.id, { status: 'completed-no-pr' }),
      );
      return { prev };
    },
    onError: (_err, _vars, context) => {
      if (context?.prev) qc.setQueryData(queryKeys.tasks.list(), context.prev);
      toast.error('Accept failed');
    },
    // SSE will reconcile
  });
}

// ---------------------------------------------------------------------------
// 3. useTaskCancel
// ---------------------------------------------------------------------------

export function useTaskCancel() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (vars: { id: number }) => cancelItem(vars.id),
    onMutate: async (vars) => {
      await qc.cancelQueries({ queryKey: queryKeys.tasks.list() });
      const prev = qc.getQueryData<TaskListResponse>(queryKeys.tasks.list());
      qc.setQueryData<TaskListResponse>(queryKeys.tasks.list(), (old) =>
        updateTaskInList(old, vars.id, { status: 'canceled' }),
      );
      return { prev };
    },
    onError: (_err, _vars, context) => {
      if (context?.prev) qc.setQueryData(queryKeys.tasks.list(), context.prev);
      toast.error('Cancel failed');
    },
  });
}

// ---------------------------------------------------------------------------
// 4. useTaskRetry
// ---------------------------------------------------------------------------

export function useTaskRetry() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (vars: { id: number }) => retryItem(vars.id),
    onMutate: async (vars) => {
      await qc.cancelQueries({ queryKey: queryKeys.tasks.list() });
      const prev = qc.getQueryData<TaskListResponse>(queryKeys.tasks.list());
      qc.setQueryData<TaskListResponse>(queryKeys.tasks.list(), (old) =>
        updateTaskInList(old, vars.id, { status: 'captain-reviewing' }),
      );
      return { prev };
    },
    onError: (_err, _vars, context) => {
      if (context?.prev) qc.setQueryData(queryKeys.tasks.list(), context.prev);
      toast.error('Retry failed');
    },
  });
}

// ---------------------------------------------------------------------------
// 4b. useResumeRateLimited
// ---------------------------------------------------------------------------

export function useResumeRateLimited() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (vars: { id: number }) => resumeRateLimited(vars.id),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: queryKeys.workers.list() });
      toast.success('Rate-limit cooldown cleared');
    },
    onError: () => {
      toast.error('Resume failed');
    },
  });
}

// ---------------------------------------------------------------------------
// 5. useTaskHandoff
// ---------------------------------------------------------------------------

export function useTaskHandoff() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (vars: { id: number }) => handoffItem(vars.id),
    onMutate: async (vars) => {
      await qc.cancelQueries({ queryKey: queryKeys.tasks.list() });
      const prev = qc.getQueryData<TaskListResponse>(queryKeys.tasks.list());
      qc.setQueryData<TaskListResponse>(queryKeys.tasks.list(), (old) =>
        updateTaskInList(old, vars.id, { status: 'handed-off' }),
      );
      return { prev };
    },
    onError: (_err, _vars, context) => {
      if (context?.prev) qc.setQueryData(queryKeys.tasks.list(), context.prev);
      toast.error('Handoff failed');
    },
  });
}

// ---------------------------------------------------------------------------
// 6. useTaskReopen
// ---------------------------------------------------------------------------

export function useTaskReopen() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (vars: { id: number; feedback: string; images?: File[] }) =>
      reopenItem(vars.id, vars.feedback, vars.images),
    onMutate: async (vars) => {
      await qc.cancelQueries({ queryKey: queryKeys.tasks.list() });
      const prev = qc.getQueryData<TaskListResponse>(queryKeys.tasks.list());
      qc.setQueryData<TaskListResponse>(queryKeys.tasks.list(), (old) =>
        updateTaskInList(old, vars.id, { status: 'queued' }),
      );
      return { prev };
    },
    onError: (_err, _vars, context) => {
      if (context?.prev) qc.setQueryData(queryKeys.tasks.list(), context.prev);
      toast.error('Reopen failed');
    },
    onSuccess: () => {
      toast.success('Task reopened');
    },
    onSettled: (_data, _err, vars) => {
      invalidateTaskDetail(qc, vars.id);
    },
  });
}

// ---------------------------------------------------------------------------
// 6b. useTaskAskReopen — reopen from Q&A with AI-synthesized feedback
// ---------------------------------------------------------------------------

export function useTaskAskReopen() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (vars: { id: number }) => askReopen(vars.id),
    onMutate: async (vars) => {
      await qc.cancelQueries({ queryKey: queryKeys.tasks.list() });
      const prev = qc.getQueryData<TaskListResponse>(queryKeys.tasks.list());
      qc.setQueryData<TaskListResponse>(queryKeys.tasks.list(), (old) =>
        updateTaskInList(old, vars.id, { status: 'queued' }),
      );
      return { prev };
    },
    onError: (_err, _vars, context) => {
      if (context?.prev) qc.setQueryData(queryKeys.tasks.list(), context.prev);
      toast.error('Reopen from Q&A failed');
    },
    onSuccess: () => {
      toast.success('Task reopened from Q&A');
    },
    onSettled: (_data, _err, vars) => {
      void qc.invalidateQueries({ queryKey: queryKeys.tasks.askHistory(vars.id) });
    },
  });
}

// ---------------------------------------------------------------------------
// 9. useTaskRework
// ---------------------------------------------------------------------------

export function useTaskRework() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (vars: { id: number; feedback: string; images?: File[] }) =>
      reworkItem(vars.id, vars.feedback, vars.images),
    onMutate: async (vars) => {
      await qc.cancelQueries({ queryKey: queryKeys.tasks.list() });
      const prev = qc.getQueryData<TaskListResponse>(queryKeys.tasks.list());
      qc.setQueryData<TaskListResponse>(queryKeys.tasks.list(), (old) =>
        updateTaskInList(old, vars.id, { status: 'rework' }),
      );
      return { prev };
    },
    onError: (_err, _vars, context) => {
      if (context?.prev) qc.setQueryData(queryKeys.tasks.list(), context.prev);
      toast.error('Rework failed');
    },
    onSuccess: () => {
      toast.success('Rework requested');
    },
    onSettled: (_data, _err, vars) => {
      invalidateTaskDetail(qc, vars.id);
    },
  });
}

// ---------------------------------------------------------------------------
// 10. useTaskMerge
// ---------------------------------------------------------------------------

export function useTaskMerge() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (vars: { id: number; prNumber: number; project: string }) => {
      const result = await mergePr(vars.prNumber, vars.project);
      // Fire-and-forget tick so captain picks up the merge
      triggerTick().catch((e) => log.warn('post-merge tick failed', e));
      return result;
    },
    onMutate: async (vars) => {
      await qc.cancelQueries({ queryKey: queryKeys.tasks.list() });
      const prev = qc.getQueryData<TaskListResponse>(queryKeys.tasks.list());
      qc.setQueryData<TaskListResponse>(queryKeys.tasks.list(), (old) =>
        updateTaskInList(old, vars.id, { status: 'captain-merging' }),
      );
      return { prev };
    },
    onError: (_err, _vars, context) => {
      if (context?.prev) qc.setQueryData(queryKeys.tasks.list(), context.prev);
      toast.error('Merge failed');
    },
    onSuccess: () => {
      toast.success('Captain will check CI and merge');
    },
  });
}

// ---------------------------------------------------------------------------
// 11. useTaskAsk
// ---------------------------------------------------------------------------

export function useTaskAsk() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (vars: { id: number; question: string; askId?: string; images?: File[] }) =>
      askTask(vars.id, vars.question, vars.askId, vars.images),
    onError: () => {
      toast.error('Ask failed');
    },
    onSettled: (_data, _err, vars) => {
      void qc.invalidateQueries({ queryKey: queryKeys.tasks.feed(vars.id) });
      void qc.invalidateQueries({ queryKey: queryKeys.tasks.askHistory(vars.id) });
    },
  });
}

// ---------------------------------------------------------------------------
// 12. useTaskAdvisor
// ---------------------------------------------------------------------------

export function useTaskAdvisor() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (vars: { id: number; message: string; intent?: string }) =>
      sendAdvisorMessage(vars.id, vars.message, vars.intent),
    onError: () => {
      toast.error('Advisor message failed');
    },
    onSettled: (_data, _err, vars) => {
      void qc.invalidateQueries({ queryKey: queryKeys.tasks.feed(vars.id) });
      void qc.invalidateQueries({ queryKey: queryKeys.tasks.askHistory(vars.id) });
    },
  });
}

// ---------------------------------------------------------------------------
// 13. useTaskNudge
// ---------------------------------------------------------------------------

export function useTaskNudge() {
  return useMutation({
    mutationFn: async (vars: { id: number; message: string; images?: File[] }) =>
      nudgeWorker(vars.id, vars.message, vars.images),
    onSuccess: (_data, vars) => {
      toast.success(`Nudged task #${vars.id}`);
    },
    onError: () => {
      toast.error('Nudge failed');
    },
  });
}

// ---------------------------------------------------------------------------
// 14. useTaskDelete
// ---------------------------------------------------------------------------

export function useTaskDelete() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (vars: { ids: number[]; opts?: { close_pr?: boolean; force?: boolean } }) =>
      deleteItems(vars.ids, vars.opts),
    onMutate: async (vars) => {
      await qc.cancelQueries({ queryKey: queryKeys.tasks.list() });
      const prev = qc.getQueryData<TaskListResponse>(queryKeys.tasks.list());
      const idSet = new Set(vars.ids);
      qc.setQueryData<TaskListResponse>(queryKeys.tasks.list(), (old) =>
        removeTasksFromList(old, idSet),
      );
      return { prev };
    },
    onError: (_err, _vars, context) => {
      if (context?.prev) qc.setQueryData(queryKeys.tasks.list(), context.prev);
      toast.error('Delete failed');
    },
    onSuccess: (result) => {
      if (result.warnings?.length) {
        for (const w of result.warnings) {
          toast.error(w);
        }
      }
    },
  });
}

// ---------------------------------------------------------------------------
// 15. useTaskClarify
// ---------------------------------------------------------------------------

export function useTaskClarify() {
  return useMutation({
    mutationFn: async (
      vars:
        | {
            id: number;
            mode: 'structured';
            answers: { question: string; answer: string }[];
            images?: File[];
          }
        | { id: number; mode: 'text'; answer: string; images?: File[] },
    ) => {
      if (vars.mode === 'structured') {
        return answerClarification(vars.id, vars.answers, vars.images);
      }
      return answerClarificationText(vars.id, vars.answer, vars.images);
    },
    onError: () => {
      toast.error('Answer failed');
    },
    // SSE handles status transition
  });
}

// ---------------------------------------------------------------------------
// 16. useTaskBulkCreate
// ---------------------------------------------------------------------------

export function useTaskBulkCreate() {
  return useMutation({
    mutationFn: async (vars: { text: string; project: string }) => {
      let titles: string[];
      try {
        const parsed = await parseTodos(vars.text, vars.project);
        titles = parsed.items;
      } catch (err) {
        log.warn('[useTaskBulkCreate] parseTodos failed, using raw text:', err);
        titles = [vars.text];
      }
      const results: { title: string; ok: boolean; error?: string }[] = [];
      for (const title of titles) {
        try {
          await addTask({ title, project: vars.project });
          results.push({ title, ok: true });
        } catch (err) {
          results.push({ title, ok: false, error: String(err) });
        }
      }
      return results;
    },
    onSuccess: (results) => {
      const ok = results.filter((r) => r.ok).length;
      const failed = results.filter((r) => !r.ok).length;
      if (ok > 0) toast.success(`Created ${ok} task${ok > 1 ? 's' : ''}`);
      if (failed > 0) toast.error(`${failed} task${failed > 1 ? 's' : ''} failed`);
    },
  });
}

// ---------------------------------------------------------------------------
// useStartImplementation — re-queue PlanReady task for normal worker
// ---------------------------------------------------------------------------

export function useStartImplementation() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (vars: { id: number; context: string }) =>
      startImplementation(vars.id, vars.context),
    onMutate: async (vars) => {
      await qc.cancelQueries({ queryKey: queryKeys.tasks.list() });
      const prev = qc.getQueryData<TaskListResponse>(queryKeys.tasks.list());
      qc.setQueryData<TaskListResponse>(queryKeys.tasks.list(), (old) =>
        updateTaskInList(old, vars.id, { status: 'queued' }),
      );
      return { prev };
    },
    onError: (_err, _vars, context) => {
      if (context?.prev) qc.setQueryData(queryKeys.tasks.list(), context.prev);
      toast.error('Start implementation failed');
    },
  });
}
