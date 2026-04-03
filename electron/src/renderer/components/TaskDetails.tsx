import React, { useCallback } from 'react';
import { apiPost } from '#renderer/api';
import { useSettingsStore } from '#renderer/stores/settingsStore';
import { useToastStore } from '#renderer/stores/toastStore';

export function TaskEmptyState(): React.ReactElement {
  const projects = useSettingsStore((s) => s.config.captain?.projects);
  const hasProjects = projects && Object.keys(projects).length > 0;

  const handleAddProject = useCallback(async () => {
    const dir = await window.mandoAPI.selectDirectory();
    if (!dir) return;
    try {
      await apiPost('/api/projects', { path: dir });
      useSettingsStore.getState().load();
    } catch (err) {
      const msg = err instanceof Error ? err.message : 'Failed to add project';
      useToastStore.getState().add('error', msg);
    }
  }, []);

  return (
    <div className="flex flex-col items-center justify-center py-16">
      <svg width="48" height="48" viewBox="0 0 48 48" fill="none" className="mb-4">
        <rect
          x="8"
          y="8"
          width="32"
          height="32"
          rx="6"
          stroke="var(--color-text-4)"
          strokeWidth="1.5"
        />
        <path
          d="M18 24l4 4 8-8"
          stroke="var(--color-text-4)"
          strokeWidth="1.5"
          strokeLinecap="round"
          strokeLinejoin="round"
        />
      </svg>
      <span className="text-subheading mb-1" style={{ color: 'var(--color-text-2)' }}>
        {hasProjects ? 'No tasks yet' : 'Add a project to get started'}
      </span>
      <span className="text-body mb-4" style={{ color: 'var(--color-text-3)' }}>
        {hasProjects
          ? 'Create a task and Captain will pick it up automatically.'
          : 'Mando needs a project folder to manage tasks.'}
      </span>
      {!hasProjects && (
        <button
          onClick={handleAddProject}
          className="text-[13px] font-semibold transition-colors hover:brightness-110 active:brightness-90"
          style={{
            padding: '8px 20px',
            borderRadius: 'var(--radius-button)',
            background: 'var(--color-accent)',
            color: 'var(--color-bg)',
            border: 'none',
            cursor: 'pointer',
          }}
        >
          Add project
        </button>
      )}
    </div>
  );
}
