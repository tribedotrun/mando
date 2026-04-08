import React, { useCallback, useState } from 'react';
import { ClipboardCheck } from 'lucide-react';
import { apiPost } from '#renderer/domains/captain/hooks/useApi';
import { useSettingsStore } from '#renderer/domains/settings';
import { toast } from 'sonner';
import { getErrorMessage } from '#renderer/utils';
import { Button } from '#renderer/components/ui/button';
import { EmptyState } from '#renderer/global/components/EmptyState';

export function TaskEmptyState(): React.ReactElement {
  const projects = useSettingsStore((s) => s.config.captain?.projects);
  const hasProjects = projects && Object.keys(projects).length > 0;
  const [adding, setAdding] = useState(false);

  const handleAddProject = useCallback(async () => {
    if (adding) return;

    let dir: string | null;
    try {
      dir = await window.mandoAPI.selectDirectory();
    } catch (err) {
      toast.error(getErrorMessage(err, 'Directory picker failed'));
      return;
    }
    if (!dir) return;

    setAdding(true);
    try {
      await apiPost('/api/projects', { path: dir });
      void useSettingsStore.getState().load();
    } catch (err) {
      toast.error(getErrorMessage(err, 'Failed to add project'));
    } finally {
      setAdding(false);
    }
  }, [adding]);

  return (
    <EmptyState
      icon={<ClipboardCheck size={48} color="var(--text-4)" strokeWidth={1} />}
      heading={hasProjects ? 'No tasks yet' : 'Add a project to get started'}
      description={
        hasProjects
          ? 'Create a task and Captain will pick it up automatically.'
          : 'Mando needs a project folder to manage tasks.'
      }
    >
      {!hasProjects && (
        <Button onClick={() => void handleAddProject()} disabled={adding}>
          {adding ? 'Adding...' : 'Add project'}
        </Button>
      )}
    </EmptyState>
  );
}
