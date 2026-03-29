import React from 'react';
import { CronJobsPanel } from '#renderer/components/CronJobsPanel';

export function SettingsScheduledTasks(): React.ReactElement {
  return <CronJobsPanel variant="settings" testId="settings-scheduled-tasks" />;
}
