import { create } from 'zustand';
import type { CronJob } from '#renderer/types';
import { fetchCron, addCronJob, runCronJob, toggleCronJob, removeCronJob } from '#renderer/api';
import { createMutate, getErrorMessage } from '#renderer/utils';

interface CronStore {
  jobs: CronJob[];
  count: number;
  loading: boolean;
  error: string | null;
  fetch: () => Promise<void>;
  add: (job: {
    name: string;
    schedule_kind: CronJob['schedule_kind'];
    schedule_value: string;
    message: string;
  }) => Promise<void>;
  runNow: (id: string) => Promise<void>;
  toggle: (id: string, enabled: boolean) => Promise<void>;
  remove: (id: string) => Promise<void>;
}

export const useCronStore = create<CronStore>((set, getState) => {
  const mutate = createMutate(getState, set);

  return {
    jobs: [],
    count: 0,
    loading: false,
    error: null,

    fetch: async () => {
      set({ loading: true, error: null });
      try {
        const data = await fetchCron();
        set({ jobs: data.jobs, count: data.count, loading: false });
      } catch (err) {
        set({ loading: false, error: getErrorMessage(err, 'Failed to fetch cron jobs') });
      }
    },

    add: (job) => mutate(() => addCronJob(job), 'Failed to add cron job'),
    runNow: (id) => mutate(() => runCronJob(id), 'Failed to run cron job'),
    toggle: (id, enabled) => mutate(() => toggleCronJob(id, enabled), 'Failed to toggle cron job'),
    remove: (id) => mutate(() => removeCronJob(id), 'Failed to remove cron job'),
  };
});
