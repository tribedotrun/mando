import React from 'react';
import { ProjectEditor } from '#renderer/domains/settings/ui/ProjectEditor';
import {
  ProjectCard,
  RemoveProjectDialog,
} from '#renderer/domains/settings/ui/SettingsProjectsParts';
import { useSettingsProjects } from '#renderer/domains/settings/runtime/useSettingsProjects';

export function SettingsProjects(): React.ReactElement {
  const {
    projects,
    editing,
    setEditing,
    removing,
    setRemoving,
    editMut,
    removeMut,
    handleSave,
    handleRemove,
  } = useSettingsProjects();

  const pathKeys = Object.keys(projects);

  return (
    <div data-testid="settings-projects" className="space-y-8">
      <h2 className="text-lg font-semibold text-foreground">Projects</h2>

      <div className="space-y-4">
        {pathKeys.length === 0 && (
          <p className="text-sm text-muted-foreground">No projects configured yet.</p>
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
                saving={editMut.isPending}
                onSave={(_k, updated) => handleSave(pathKey, project, updated)}
                onCancel={() => setEditing(null)}
              />
            );
          }
          return (
            <ProjectCard
              key={pathKey}
              pathKey={pathKey}
              project={project}
              removePending={removeMut.isPending}
              onEdit={setEditing}
              onRemove={setRemoving}
            />
          );
        })}
      </div>

      {removing && projects[removing] && (
        <RemoveProjectDialog
          project={projects[removing]}
          isPending={removeMut.isPending}
          onConfirm={() => handleRemove(removing)}
          onCancel={() => setRemoving(null)}
        />
      )}
    </div>
  );
}
