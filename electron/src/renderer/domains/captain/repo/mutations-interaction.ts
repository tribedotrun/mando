import { useMutation, useQueryClient } from '@tanstack/react-query';
import log from '#renderer/global/service/logger';
import {
  mergePr,
  triggerTick,
  askTask,
  nudgeWorker,
  deleteItems,
  answerClarification,
  answerClarificationText,
  sendAdvisorMessage,
} from '#renderer/domains/captain/repo/api';
import type { TaskListResponse } from '#renderer/global/types';
import { queryKeys } from '#renderer/global/repo/queryKeys';
import { toReactQuery } from '#result';
import {
  removeTasksFromList,
  updateTaskInList,
} from '#renderer/domains/captain/repo/taskListHelpers';

async function triggerPostMergeTick(): Promise<void> {
  try {
    await toReactQuery(triggerTick());
  } catch (e) {
    log.warn('post-merge tick failed', e);
  }
}

export function useTaskMerge() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (vars: { id: number; prNumber: number; project: string }) => {
      const result = await toReactQuery(mergePr(vars.prNumber, vars.project));
      void triggerPostMergeTick();
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
    onError: (err, _vars, context) => {
      if (context?.prev) qc.setQueryData(queryKeys.tasks.list(), context.prev);
      log.error('useTaskMerge', err);
    },
  });
}

export function useTaskAsk() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (vars: { id: number; question: string; askId?: string; images?: File[] }) =>
      toReactQuery(askTask(vars.id, vars.question, vars.askId, vars.images)),
    onError: (err) => {
      log.error('useTaskAsk', err);
    },
    onSettled: (_data, err, vars) => {
      if (err) log.warn('useTaskAsk settled with error', err);
      void qc.invalidateQueries({ queryKey: queryKeys.tasks.feed(vars.id) });
      void qc.invalidateQueries({ queryKey: queryKeys.tasks.askHistory(vars.id) });
    },
  });
}

export function useTaskAdvisor() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (vars: { id: number; message: string; intent?: string }) =>
      toReactQuery(sendAdvisorMessage(vars.id, vars.message, vars.intent)),
    onError: (err) => {
      log.error('useTaskAdvisor', err);
    },
    onSettled: (_data, err, vars) => {
      if (err) log.warn('useTaskAdvisor settled with error', err);
      void qc.invalidateQueries({ queryKey: queryKeys.tasks.feed(vars.id) });
      void qc.invalidateQueries({ queryKey: queryKeys.tasks.askHistory(vars.id) });
    },
  });
}

export function useTaskNudge() {
  return useMutation({
    mutationFn: (vars: { id: number; message: string; images?: File[] }) =>
      toReactQuery(nudgeWorker(vars.id, vars.message, vars.images)),
    onError: (err) => {
      log.error('useTaskNudge', err);
    },
  });
}

export function useTaskDelete() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (vars: { ids: number[]; opts?: { close_pr?: boolean; force?: boolean } }) =>
      toReactQuery(deleteItems(vars.ids, vars.opts)),
    onMutate: async (vars) => {
      await qc.cancelQueries({ queryKey: queryKeys.tasks.list() });
      const prev = qc.getQueryData<TaskListResponse>(queryKeys.tasks.list());
      const idSet = new Set(vars.ids);
      qc.setQueryData<TaskListResponse>(queryKeys.tasks.list(), (old) =>
        removeTasksFromList(old, idSet),
      );
      return { prev };
    },
    onError: (err, _vars, context) => {
      if (context?.prev) qc.setQueryData(queryKeys.tasks.list(), context.prev);
      log.error('useTaskDelete', err);
    },
  });
}

export function useTaskClarify() {
  return useMutation({
    mutationFn: (
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
        return toReactQuery(answerClarification(vars.id, vars.answers, vars.images));
      }
      return toReactQuery(answerClarificationText(vars.id, vars.answer, vars.images));
    },
    onError: (err) => {
      log.error('useTaskClarify', err);
    },
  });
}
