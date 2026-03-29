import { create } from 'zustand';
import type { CronJob } from '#renderer/types';
import { fetchCron, addCronJob, runCronJob, toggleCronJob, removeCronJob } from '#renderer/api';
import { getErrorMessage } from '#renderer/utils';

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

export const useCronStore = create<CronStore>((set, getState) => ({
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
      set({
        loading: false,
        error: getErrorMessage(err, 'Failed to fetch cron jobs'),
      });
    }
  },

  add: async (job) => {
    try {
      await addCronJob(job);
      await getState().fetch();
    } catch (err) {
      set({
        error: getErrorMessage(err, 'Failed to add cron job'),
      });
      throw err;
    }
  },

  runNow: async (id: string) => {
    try {
      await runCronJob(id);
      await getState().fetch();
    } catch (err) {
      set({
        error: getErrorMessage(err, 'Failed to run cron job'),
      });
      throw err;
    }
  },

  toggle: async (id: string, enabled: boolean) => {
    try {
      await toggleCronJob(id, enabled);
      await getState().fetch();
    } catch (err) {
      set({
        error: getErrorMessage(err, 'Failed to toggle cron job'),
      });
      throw err;
    }
  },

  remove: async (id: string) => {
    try {
      await removeCronJob(id);
      await getState().fetch();
    } catch (err) {
      set({
        error: getErrorMessage(err, 'Failed to remove cron job'),
      });
      throw err;
    }
  },
}));
