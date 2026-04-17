import React, { useState } from 'react';
import { Card, CardContent } from '#renderer/global/ui/card';
import { Button } from '#renderer/global/ui/button';
import { Badge } from '#renderer/global/ui/badge';
import {
  AlertDialog,
  AlertDialogContent,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogCancel,
  AlertDialogAction,
} from '#renderer/global/ui/alert-dialog';
import { ProjectEditor } from '#renderer/domains/settings/ui/ProjectEditor';
import {
  useConfig,
  useProjectEdit,
  useProjectRemove,
} from '#renderer/domains/settings/runtime/hooks';
import type { ProjectConfig } from '#renderer/global/types';
import { projectLogoUrl } from '#renderer/domains/settings/runtime/useApi';

const EMPTY_PROJECTS: Record<string, ProjectConfig> = {};

export function SettingsProjects(): React.ReactElement {
  const { data: config } = useConfig();
  const editMut = useProjectEdit();
  const removeMut = useProjectRemove();
  const projects = config?.captain?.projects ?? EMPTY_PROJECTS;

  const [editing, setEditing] = useState<string | null>(null);
  const [removing, setRemoving] = useState<string | null>(null);

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
                onSave={(_k, updated) => {
                  const currentName = project.name;
                  editMut.mutate(
                    {
                      currentName,
                      rename: updated.name !== currentName ? updated.name : undefined,
                      github_repo: updated.githubRepo || undefined,
                      clear_github_repo:
                        !updated.githubRepo && !!project.githubRepo ? true : undefined,
                      aliases: updated.aliases ?? [],
                      hooks: updated.hooks ?? {},
                      preamble: updated.workerPreamble ?? '',
                      check_command: updated.checkCommand ?? '',
                      scout_summary: updated.scoutSummary ?? '',
                    },
                    { onSuccess: () => setEditing(null) },
                  );
                }}
                onCancel={() => setEditing(null)}
              />
            );
          }
          const displayName = project.name || pathKey;
          return (
            <Card key={pathKey} data-testid={`project-card-${displayName}`} className="py-4">
              <CardContent>
                <div className="flex items-start justify-between">
                  <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-2">
                      {project.logo && (
                        <img
                          src={projectLogoUrl(project.logo)}
                          alt=""
                          width={20}
                          height={20}
                          className="shrink-0 rounded object-contain"
                          onError={(e) => {
                            (e.target as HTMLImageElement).style.display = 'none';
                          }}
                        />
                      )}
                      <h3
                        className="min-w-0 truncate text-sm font-semibold text-foreground"
                        title={displayName}
                      >
                        {displayName}
                      </h3>
                    </div>
                    <p className="mt-1 truncate font-mono text-xs text-muted-foreground">
                      {project.path}
                    </p>
                    {project.scoutSummary && (
                      <p className="mt-1 break-words text-xs text-muted-foreground">
                        {project.scoutSummary}
                      </p>
                    )}
                    {project.githubRepo && (
                      <p
                        className="mt-1 truncate text-xs text-muted-foreground"
                        title={project.githubRepo}
                      >
                        {project.githubRepo}
                      </p>
                    )}
                    {project.aliases && project.aliases.length > 0 && (
                      <div className="mt-2 flex flex-wrap gap-1">
                        {project.aliases.map((a) => (
                          <Badge key={a} variant="secondary" className="text-xs">
                            {a}
                          </Badge>
                        ))}
                      </div>
                    )}
                    {project.hooks && Object.keys(project.hooks).length > 0 && (
                      <div
                        className="mt-2 truncate text-xs text-muted-foreground"
                        title={`Hooks: ${Object.keys(project.hooks).join(', ')}`}
                      >
                        Hooks: {Object.keys(project.hooks).join(', ')}
                      </div>
                    )}
                  </div>
                  <div className="ml-4 flex items-center gap-2">
                    <Button
                      data-testid={`project-edit-${displayName}`}
                      variant="ghost"
                      size="xs"
                      onClick={() => setEditing(pathKey)}
                    >
                      Edit
                    </Button>
                    <Button
                      data-testid={`project-remove-${displayName}`}
                      variant="destructive"
                      size="xs"
                      disabled={removeMut.isPending}
                      onClick={() => setRemoving(pathKey)}
                    >
                      Remove
                    </Button>
                  </div>
                </div>
              </CardContent>
            </Card>
          );
        })}
      </div>

      {removing && projects[removing] && (
        <AlertDialog open onOpenChange={() => setRemoving(null)}>
          <AlertDialogContent size="sm">
            <AlertDialogHeader>
              <AlertDialogTitle>Remove project</AlertDialogTitle>
              <AlertDialogDescription>
                Remove {projects[removing].name}? All tasks belonging to this project will be
                deleted.
              </AlertDialogDescription>
            </AlertDialogHeader>
            <AlertDialogFooter>
              <AlertDialogCancel>Cancel</AlertDialogCancel>
              <AlertDialogAction
                variant="destructive"
                disabled={removeMut.isPending}
                onClick={() => {
                  removeMut.mutate(
                    { name: projects[removing].name },
                    { onSuccess: () => setRemoving(null) },
                  );
                }}
              >
                {removeMut.isPending ? 'Removing...' : 'Remove'}
              </AlertDialogAction>
            </AlertDialogFooter>
          </AlertDialogContent>
        </AlertDialog>
      )}
    </div>
  );
}
