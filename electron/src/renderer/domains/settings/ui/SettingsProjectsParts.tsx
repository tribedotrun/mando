import React from 'react';
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
} from '#renderer/global/ui/alert-dialog';
import type { ProjectConfig } from '#renderer/global/types';
import { projectLogoUrl } from '#renderer/domains/settings/runtime/useApi';

interface ProjectCardProps {
  pathKey: string;
  project: ProjectConfig;
  removePending: boolean;
  onEdit: (pathKey: string) => void;
  onRemove: (pathKey: string) => void;
}

export function ProjectCard({
  pathKey,
  project,
  removePending,
  onEdit,
  onRemove,
}: ProjectCardProps): React.ReactElement {
  const displayName = project.name || pathKey;
  return (
    <Card data-testid={`project-card-${displayName}`} className="py-4">
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
            <p className="mt-1 truncate font-mono text-xs text-muted-foreground">{project.path}</p>
            {project.scoutSummary && (
              <p className="mt-1 break-words text-xs text-muted-foreground">
                {project.scoutSummary}
              </p>
            )}
            {project.githubRepo && (
              <p className="mt-1 truncate text-xs text-muted-foreground" title={project.githubRepo}>
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
              onClick={() => onEdit(pathKey)}
            >
              Edit
            </Button>
            <Button
              data-testid={`project-remove-${displayName}`}
              variant="destructive"
              size="xs"
              disabled={removePending}
              onClick={() => onRemove(pathKey)}
            >
              Remove
            </Button>
          </div>
        </div>
      </CardContent>
    </Card>
  );
}

interface RemoveProjectDialogProps {
  project: ProjectConfig;
  isPending: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}

export function RemoveProjectDialog({
  project,
  isPending,
  onConfirm,
  onCancel,
}: RemoveProjectDialogProps): React.ReactElement {
  return (
    <AlertDialog open onOpenChange={onCancel}>
      <AlertDialogContent size="sm">
        <AlertDialogHeader>
          <AlertDialogTitle>Remove project</AlertDialogTitle>
          <AlertDialogDescription>
            Remove {project.name}? All tasks belonging to this project will be deleted.
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel disabled={isPending}>Cancel</AlertDialogCancel>
          <Button variant="destructive" size="sm" disabled={isPending} onClick={onConfirm}>
            {isPending ? 'Removing...' : 'Remove'}
          </Button>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
