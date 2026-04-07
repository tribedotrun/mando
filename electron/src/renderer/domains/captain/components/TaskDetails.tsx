import React, { useCallback, useState } from 'react';
import { ClipboardCheck } from 'lucide-react';
import { apiPost } from '#renderer/domains/captain/hooks/useApi';
import { useSettingsStore } from '#renderer/domains/settings';
import { toast } from 'sonner';
import { getErrorMessage } from '#renderer/utils';
import { Button } from '#renderer/components/ui/button';

export function TaskEmptyState(): React.ReactElement {
  const projects = useSettingsStore((s) => s.config.captain?.projects);
  const hasProjects = projects && Object.keys(projects).length > 0;
  const [adding, setAdding] = useState(false);

  const handleAddProject = useCallback(async () => {
    if (adding) return;

    // Run the picker outside the loading state so the button doesn't flash
    // "Adding..." while the user is still browsing. Errors from the picker
    // itself (IPC failure) are surfaced as a toast.
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
      useSettingsStore.getState().load();
    } catch (err) {
      toast.error(getErrorMessage(err, 'Failed to add project'));
    } finally {
      setAdding(false);
    }
  }, [adding]);

  return (
    <div className="flex flex-col items-center justify-center py-16">
      <ClipboardCheck size={48} color="var(--text-4)" strokeWidth={1} className="mb-4" />
      <span className="text-subheading mb-1 text-muted-foreground">
        {hasProjects ? 'No tasks yet' : 'Add a project to get started'}
      </span>
      <span className="text-body mb-4 text-text-3">
        {hasProjects
          ? 'Create a task and Captain will pick it up automatically.'
          : 'Mando needs a project folder to manage tasks.'}
      </span>
      {!hasProjects && (
        <Button onClick={handleAddProject} disabled={adding}>
          {adding ? 'Adding...' : 'Add project'}
        </Button>
      )}
    </div>
  );
}
