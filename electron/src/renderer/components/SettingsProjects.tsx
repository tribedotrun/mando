import React, { useState } from 'react';
import { cardStyle } from '#renderer/styles';
import { ProjectEditor } from '#renderer/components/ProjectEditor';
import { useSettingsStore } from '#renderer/stores/settingsStore';
import type { ProjectConfig } from '#renderer/stores/settingsStore';

const EMPTY_PROJECTS: Record<string, ProjectConfig> = {};

export function SettingsProjects(): React.ReactElement {
  const projects = useSettingsStore((s) => s.config.captain?.projects ?? EMPTY_PROJECTS);
  const updateProject = useSettingsStore((s) => s.updateProject);
  const removeProject = useSettingsStore((s) => s.removeProject);
  const save = useSettingsStore((s) => s.save);

  const [editing, setEditing] = useState<string | null>(null);

  const pathKeys = Object.keys(projects);

  return (
    <div data-testid="settings-projects" className="space-y-8">
      <div>
        <h2 className="text-lg font-semibold" style={{ color: 'var(--color-text-1)' }}>
          Projects
        </h2>
        <p className="mt-1 text-sm" style={{ color: 'var(--color-text-3)' }}>
          Manage projects Mando can work on.
        </p>
      </div>

      <div className="space-y-4">
        {pathKeys.length === 0 && (
          <p className="text-sm" style={{ color: 'var(--color-text-3)' }}>
            No projects configured yet.
          </p>
        )}

        {pathKeys.map((pathKey) => {
          const project = projects[pathKey];
          if (editing === pathKey) {
            return (
              <ProjectEditor
                key={pathKey}
                pathKey={pathKey}
                project={project}
                existingProjects={projects}
                onSave={(k, r) => {
                  updateProject(k, r);
                  save();
                  setEditing(null);
                }}
                onCancel={() => setEditing(null)}
              />
            );
          }
          const displayName = project.name || pathKey;
          return (
            <div key={pathKey} data-testid={`project-card-${displayName}`} style={cardStyle}>
              <div className="flex items-start justify-between">
                <div className="min-w-0 flex-1">
                  <h3 className="text-sm font-semibold" style={{ color: 'var(--color-text-1)' }}>
                    {displayName}
                  </h3>
                  <p
                    className="mt-1 truncate font-mono text-xs"
                    style={{ color: 'var(--color-text-3)' }}
                  >
                    {project.path}
                  </p>
                  {project.scoutSummary && (
                    <p className="mt-1 text-xs" style={{ color: 'var(--color-text-2)' }}>
                      {project.scoutSummary}
                    </p>
                  )}
                  {project.githubRepo && (
                    <p className="mt-1 text-xs" style={{ color: 'var(--color-text-3)' }}>
                      {project.githubRepo}
                    </p>
                  )}
                  {project.aliases && project.aliases.length > 0 && (
                    <div className="mt-2 flex flex-wrap gap-1">
                      {project.aliases.map((a) => (
                        <span
                          key={a}
                          className="rounded px-2 py-0.5 text-xs"
                          style={{
                            background: 'var(--color-surface-2)',
                            color: 'var(--color-text-2)',
                          }}
                        >
                          {a}
                        </span>
                      ))}
                    </div>
                  )}
                  {project.hooks && Object.keys(project.hooks).length > 0 && (
                    <div className="mt-2 text-xs" style={{ color: 'var(--color-text-3)' }}>
                      Hooks: {Object.keys(project.hooks).join(', ')}
                    </div>
                  )}
                </div>
                <div className="ml-4 flex items-center gap-2">
                  <button
                    data-testid={`project-edit-${displayName}`}
                    onClick={() => setEditing(pathKey)}
                    className="rounded-md px-3 py-1.5 text-xs"
                    style={{ color: 'var(--color-text-2)' }}
                  >
                    Edit
                  </button>
                  <button
                    data-testid={`project-remove-${displayName}`}
                    onClick={() => {
                      removeProject(pathKey);
                      save();
                    }}
                    className="rounded-md px-3 py-1.5 text-xs"
                    style={{ color: 'var(--color-error)' }}
                  >
                    Remove
                  </button>
                </div>
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
