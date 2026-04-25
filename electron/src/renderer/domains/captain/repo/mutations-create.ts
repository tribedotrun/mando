import { useMutation } from '@tanstack/react-query';
import log from '#renderer/global/service/logger';
import { addTask, parseTodos, type AddTaskInput } from '#renderer/domains/captain/repo/api';
import { toReactQuery } from '#result';

export function useTaskCreate() {
  return useMutation({
    mutationFn: (input: AddTaskInput) => toReactQuery(addTask(input)),
  });
}

export function useTaskBulkCreate() {
  return useMutation({
    mutationFn: async (vars: { text: string; project: string }) => {
      let titles: string[];
      try {
        const parsed = await toReactQuery(parseTodos(vars.text, vars.project));
        titles = parsed.items;
      } catch (err) {
        log.warn('[useTaskBulkCreate] parseTodos failed, using raw text:', err);
        titles = [vars.text];
      }
      const results: { title: string; ok: boolean; error?: string }[] = [];
      for (const title of titles) {
        try {
          await toReactQuery(addTask({ title, project: vars.project }));
          results.push({ title, ok: true });
        } catch (err) {
          results.push({ title, ok: false, error: String(err) });
        }
      }
      return results;
    },
  });
}
