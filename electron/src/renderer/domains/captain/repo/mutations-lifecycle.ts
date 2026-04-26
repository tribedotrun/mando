import { useMutation, useQueryClient } from '@tanstack/react-query';
import log from '#renderer/global/service/logger';
import {
  acceptItem,
  cancelItem,
  retryItem,
  resumeRateLimited,
  handoffItem,
  stopItem,
  reopenItem,
  reworkItem,
  askReopen,
  setTaskIsBugFix,
  startImplementation,
} from '#renderer/domains/captain/repo/api';
import type { TaskListResponse } from '#renderer/global/types';
import { queryKeys } from '#renderer/global/repo/queryKeys';
import { invalidateTaskDetail } from '#renderer/domains/captain/repo/taskDetailInvalidation';
import { toReactQuery } from '#result';
import { updateTaskInList } from '#renderer/domains/captain/repo/taskListHelpers';

export function useTaskAccept() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (vars: { id: number }) => toReactQuery(acceptItem(vars.id)),
    onMutate: async (vars) => {
      await qc.cancelQueries({ queryKey: queryKeys.tasks.list() });
      const prev = qc.getQueryData<TaskListResponse>(queryKeys.tasks.list());
      qc.setQueryData<TaskListResponse>(queryKeys.tasks.list(), (old) =>
        updateTaskInList(old, vars.id, { status: 'completed-no-pr' }),
      );
      return { prev };
    },
    onError: (err, _vars, context) => {
      if (context?.prev) qc.setQueryData(queryKeys.tasks.list(), context.prev);
      log.error('useTaskAccept', err);
    },
  });
}

export function useTaskCancel() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (vars: { id: number }) => toReactQuery(cancelItem(vars.id)),
    onMutate: async (vars) => {
      await qc.cancelQueries({ queryKey: queryKeys.tasks.list() });
      const prev = qc.getQueryData<TaskListResponse>(queryKeys.tasks.list());
      qc.setQueryData<TaskListResponse>(queryKeys.tasks.list(), (old) =>
        updateTaskInList(old, vars.id, { status: 'canceled' }),
      );
      return { prev };
    },
    onError: (err, _vars, context) => {
      if (context?.prev) qc.setQueryData(queryKeys.tasks.list(), context.prev);
      log.error('useTaskCancel', err);
    },
  });
}

export function useTaskRetry() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (vars: { id: number }) => toReactQuery(retryItem(vars.id)),
    onMutate: async (vars) => {
      await qc.cancelQueries({ queryKey: queryKeys.tasks.list() });
      const prev = qc.getQueryData<TaskListResponse>(queryKeys.tasks.list());
      qc.setQueryData<TaskListResponse>(queryKeys.tasks.list(), (old) =>
        updateTaskInList(old, vars.id, { status: 'captain-reviewing' }),
      );
      return { prev };
    },
    onError: (err, _vars, context) => {
      if (context?.prev) qc.setQueryData(queryKeys.tasks.list(), context.prev);
      log.error('useTaskRetry', err);
    },
  });
}

export function useResumeRateLimited() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (vars: { id: number }) => toReactQuery(resumeRateLimited(vars.id)),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: queryKeys.workers.list() });
    },
    onError: (err) => {
      log.error('useResumeRateLimited', err);
    },
  });
}

export function useTaskHandoff() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (vars: { id: number }) => toReactQuery(handoffItem(vars.id)),
    onMutate: async (vars) => {
      await qc.cancelQueries({ queryKey: queryKeys.tasks.list() });
      const prev = qc.getQueryData<TaskListResponse>(queryKeys.tasks.list());
      qc.setQueryData<TaskListResponse>(queryKeys.tasks.list(), (old) =>
        updateTaskInList(old, vars.id, { status: 'handed-off' }),
      );
      return { prev };
    },
    onError: (err, _vars, context) => {
      if (context?.prev) qc.setQueryData(queryKeys.tasks.list(), context.prev);
      log.error('useTaskHandoff', err);
    },
  });
}

export function useTaskStop() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (vars: { id: number }) => toReactQuery(stopItem(vars.id)),
    onMutate: async (vars) => {
      await qc.cancelQueries({ queryKey: queryKeys.tasks.list() });
      const prev = qc.getQueryData<TaskListResponse>(queryKeys.tasks.list());
      qc.setQueryData<TaskListResponse>(queryKeys.tasks.list(), (old) =>
        updateTaskInList(old, vars.id, { status: 'stopped' }),
      );
      return { prev };
    },
    onError: (err, _vars, context) => {
      if (context?.prev) qc.setQueryData(queryKeys.tasks.list(), context.prev);
      log.error('useTaskStop', err);
    },
  });
}

export function useTaskReopen() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (vars: { id: number; feedback: string; images?: File[] }) =>
      toReactQuery(reopenItem(vars.id, vars.feedback, vars.images)),
    onMutate: async (vars) => {
      await qc.cancelQueries({ queryKey: queryKeys.tasks.list() });
      const prev = qc.getQueryData<TaskListResponse>(queryKeys.tasks.list());
      qc.setQueryData<TaskListResponse>(queryKeys.tasks.list(), (old) =>
        updateTaskInList(old, vars.id, { status: 'queued' }),
      );
      return { prev };
    },
    onError: (err, _vars, context) => {
      if (context?.prev) qc.setQueryData(queryKeys.tasks.list(), context.prev);
      log.error('useTaskReopen', err);
    },
    onSettled: (_data, err, vars) => {
      if (err) log.warn('useTaskReopen settled with error', err);
      invalidateTaskDetail(qc, vars.id);
    },
  });
}

export function useTaskAskReopen() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (vars: { id: number }) => toReactQuery(askReopen(vars.id)),
    onMutate: async (vars) => {
      await qc.cancelQueries({ queryKey: queryKeys.tasks.list() });
      const prev = qc.getQueryData<TaskListResponse>(queryKeys.tasks.list());
      qc.setQueryData<TaskListResponse>(queryKeys.tasks.list(), (old) =>
        updateTaskInList(old, vars.id, { status: 'queued' }),
      );
      return { prev };
    },
    onError: (err, _vars, context) => {
      if (context?.prev) qc.setQueryData(queryKeys.tasks.list(), context.prev);
      log.error('useTaskAskReopen', err);
    },
    onSettled: (_data, err, vars) => {
      if (err) log.warn('useTaskAskReopen settled with error', err);
      void qc.invalidateQueries({ queryKey: queryKeys.tasks.askHistory(vars.id) });
    },
  });
}

export function useTaskRework() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (vars: { id: number; feedback: string; images?: File[] }) =>
      toReactQuery(reworkItem(vars.id, vars.feedback, vars.images)),
    onMutate: async (vars) => {
      await qc.cancelQueries({ queryKey: queryKeys.tasks.list() });
      const prev = qc.getQueryData<TaskListResponse>(queryKeys.tasks.list());
      qc.setQueryData<TaskListResponse>(queryKeys.tasks.list(), (old) =>
        updateTaskInList(old, vars.id, { status: 'rework' }),
      );
      return { prev };
    },
    onError: (err, _vars, context) => {
      if (context?.prev) qc.setQueryData(queryKeys.tasks.list(), context.prev);
      log.error('useTaskRework', err);
    },
    onSettled: (_data, err, vars) => {
      if (err) log.warn('useTaskRework settled with error', err);
      invalidateTaskDetail(qc, vars.id);
    },
  });
}

export function useTaskSetIsBugFix() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (vars: { id: number; value: boolean }) =>
      toReactQuery(setTaskIsBugFix(vars.id, vars.value)),
    onMutate: async (vars) => {
      await qc.cancelQueries({ queryKey: queryKeys.tasks.list() });
      const prev = qc.getQueryData<TaskListResponse>(queryKeys.tasks.list());
      qc.setQueryData<TaskListResponse>(queryKeys.tasks.list(), (old) =>
        updateTaskInList(old, vars.id, { is_bug_fix: vars.value }),
      );
      return { prev };
    },
    onError: (err, _vars, context) => {
      if (context?.prev) qc.setQueryData(queryKeys.tasks.list(), context.prev);
      log.error('useTaskSetIsBugFix', err);
    },
    onSettled: (_data, err, vars) => {
      if (err) log.warn('useTaskSetIsBugFix settled with error', err);
      invalidateTaskDetail(qc, vars.id);
    },
  });
}

export function useStartImplementation() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (vars: { id: number; context: string }) =>
      toReactQuery(startImplementation(vars.id, vars.context)),
    onMutate: async (vars) => {
      await qc.cancelQueries({ queryKey: queryKeys.tasks.list() });
      const prev = qc.getQueryData<TaskListResponse>(queryKeys.tasks.list());
      qc.setQueryData<TaskListResponse>(queryKeys.tasks.list(), (old) =>
        updateTaskInList(old, vars.id, { status: 'queued' }),
      );
      return { prev };
    },
    onError: (err, _vars, context) => {
      if (context?.prev) qc.setQueryData(queryKeys.tasks.list(), context.prev);
      log.error('useStartImplementation', err);
    },
  });
}
