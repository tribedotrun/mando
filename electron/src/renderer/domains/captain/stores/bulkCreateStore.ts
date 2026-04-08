import { create } from 'zustand';
import { toast } from 'sonner';
import { addTask, parseTodos, type AddTaskInput } from '#renderer/api';
import { useTaskStore } from '#renderer/domains/captain/stores/taskStore';
import { getErrorMessage } from '#renderer/utils';
import log from '#renderer/logger';

type Phase = 'idle' | 'active';

interface TaskCreateStore {
  phase: Phase;
  startSingle: (input: AddTaskInput) => void;
  startBulk: (text: string, project?: string) => void;
}

let gen = 0;

export const useTaskCreateStore = create<TaskCreateStore>((set) => ({
  phase: 'idle',

  startSingle: (input) => {
    const myGen = ++gen;
    set({ phase: 'active' });
    const toastId = toast.loading('Parsing task...');

    void (async () => {
      let title = input.title;
      try {
        const { items } = await parseTodos(title, input.project);
        if (items.length > 0 && items[0].trim()) title = items[0].trim();
      } catch (err) {
        log.warn('[TaskCreate] AI parse failed, using raw title', err);
      }

      if (myGen !== gen) return;
      toast.loading('Creating task...', { id: toastId });
      try {
        await addTask({ ...input, title });
        if (myGen !== gen) return;
        await useTaskStore.getState().fetch();
        toast.success('Added task', { id: toastId });
      } catch (err) {
        if (myGen !== gen) return;
        toast.error(getErrorMessage(err, 'Failed to create task'), { id: toastId });
      } finally {
        if (myGen === gen) set({ phase: 'idle' });
      }
    })();
  },

  startBulk: (text, project) => {
    const myGen = ++gen;
    set({ phase: 'active' });
    const toastId = toast.loading('Parsing tasks...');

    void (async () => {
      try {
        const { items } = await parseTodos(text, project);
        if (items.length === 0) {
          toast.error('No tasks found in the text.', { id: toastId });
          set({ phase: 'idle' });
          return;
        }

        const failures: string[] = [];
        let firstFailureMessage: string | null = null;
        for (let i = 0; i < items.length; i++) {
          if (myGen !== gen) return;
          toast.loading(`Adding ${i}/${items.length}...`, { id: toastId });
          try {
            await addTask({ title: items[i], project });
          } catch (itemErr) {
            const msg = getErrorMessage(itemErr, 'Failed to create task');
            log.warn('[BulkCreate] addTask failed', { title: items[i], error: msg });
            failures.push(items[i]);
            if (firstFailureMessage === null) firstFailureMessage = msg;
          }
        }

        if (myGen !== gen) return;
        await useTaskStore.getState().fetch();

        if (failures.length > 0) {
          const detail = firstFailureMessage ? `: ${firstFailureMessage}` : '';
          toast.error(`${failures.length} of ${items.length} failed${detail}`, { id: toastId });
        } else {
          toast.success(`Added ${items.length} task${items.length === 1 ? '' : 's'}`, {
            id: toastId,
          });
        }
      } catch (err) {
        if (myGen !== gen) return;
        toast.error(getErrorMessage(err, 'Bulk create failed'), { id: toastId });
      } finally {
        if (myGen === gen) set({ phase: 'idle' });
      }
    })();
  },
}));
