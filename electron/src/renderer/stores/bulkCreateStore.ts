import { create } from 'zustand';
import { addTask, parseBulkTodos } from '#renderer/api';
import { useTaskStore } from '#renderer/stores/taskStore';

export type BulkCreatePhase =
  | { step: 'idle' }
  | { step: 'parsing' }
  | { step: 'creating'; done: number; total: number }
  | { step: 'done'; count: number }
  | { step: 'error'; message: string };

interface BulkCreateStore {
  phase: BulkCreatePhase;
  start: (text: string, project?: string) => void;
  dismiss: () => void;
}

let autoDismissTimer: ReturnType<typeof setTimeout> | undefined;

export const useBulkCreateStore = create<BulkCreateStore>((set, get) => ({
  phase: { step: 'idle' },

  start: (text, project) => {
    clearTimeout(autoDismissTimer);
    set({ phase: { step: 'parsing' } });

    void (async () => {
      try {
        const items = await parseBulkTodos(text);
        if (items.length === 0) {
          set({ phase: { step: 'error', message: 'No tasks found in the text.' } });
          return;
        }

        set({ phase: { step: 'creating', done: 0, total: items.length } });
        const failures: string[] = [];
        for (let i = 0; i < items.length; i++) {
          try {
            await addTask({ title: items[i], project });
          } catch {
            failures.push(items[i]);
          }
          set({ phase: { step: 'creating', done: i + 1, total: items.length } });
        }

        await useTaskStore.getState().fetch();

        if (failures.length > 0) {
          set({
            phase: {
              step: 'error',
              message: `${failures.length} of ${items.length} failed`,
            },
          });
        } else {
          set({ phase: { step: 'done', count: items.length } });
          autoDismissTimer = setTimeout(() => get().dismiss(), 3000);
        }
      } catch (err) {
        set({
          phase: {
            step: 'error',
            message: err instanceof Error ? err.message : 'Bulk create failed',
          },
        });
      }
    })();
  },

  dismiss: () => set({ phase: { step: 'idle' } }),
}));
