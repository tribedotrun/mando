import React from 'react';
import { ProjectEditor } from '#renderer/domains/settings/ui/ProjectEditor';
import {
  ProjectCard,
  RemoveProjectDialog,
} from '#renderer/domains/settings/ui/SettingsProjectsParts';
import { useSettingsProjects } from '#renderer/domains/settings/runtime/useSettingsProjects';

export function SettingsProjects(): React.ReactElement {
  const settings = useSettingsProjects();

  const pathKeys = Object.keys(settings.projects.items);
  const removing = settings.removing.value;

  return (
    <div data-testid="settings-projects" className="space-y-8">
      <h2 className="text-lg font-semibold text-foreground">Projects</h2>

      <div className="space-y-4">
        {pathKeys.length === 0 && (
          <p className="text-sm text-muted-foreground">No projects configured yet.</p>
        )}

        {pathKeys.map((pathKey) => {
          const project = settings.projects.items[pathKey];
          if (settings.editing.value === pathKey) {
            return (
              <ProjectEditor
                key={pathKey}
                pathKey={pathKey}
                project={project}
                existingProjects={settings.projects.items}
                saving={settings.mutations.editMut.isPending}
                onSave={(_k, updated) => settings.actions.handleSave(pathKey, project, updated)}
                onCancel={() => settings.editing.set(null)}
              />
            );
          }
          return (
            <ProjectCard
              key={pathKey}
              pathKey={pathKey}
              project={project}
              removePending={settings.mutations.removeMut.isPending}
              onEdit={settings.editing.set}
              onRemove={settings.removing.set}
            />
          );
        })}
      </div>

      {removing && settings.projects.items[removing] && (
        <RemoveProjectDialog
          project={settings.projects.items[removing]}
          isPending={settings.mutations.removeMut.isPending}
          onConfirm={() => settings.actions.handleRemove(removing)}
          onCancel={() => settings.removing.set(null)}
        />
      )}
    </div>
  );
}
