import React from 'react';
import { Card, CardContent } from '#renderer/global/ui/primitives/card';
import { Button } from '#renderer/global/ui/primitives/button';
import { ProjectHooksFields } from '#renderer/domains/settings/ui/ProjectHooksFields';
import { ProjectLogoField } from '#renderer/domains/settings/ui/ProjectLogoField';
import { ProjectEditorFields } from '#renderer/domains/settings/ui/ProjectEditorFields';
import type { ProjectConfig } from '#renderer/global/types';
import { useProjectEditor } from '#renderer/domains/settings/runtime/useProjectEditor';

export interface ProjectEditorProps {
  pathKey: string;
  project: ProjectConfig;
  existingProjects: Record<string, ProjectConfig>;
  onSave: (pathKey: string, project: ProjectConfig) => void;
  onCancel: () => void;
  saving?: boolean;
}

export function ProjectEditor({
  pathKey,
  project,
  existingProjects,
  onSave,
  onCancel,
  saving,
}: ProjectEditorProps): React.ReactElement {
  const editor = useProjectEditor({ pathKey, project, existingProjects, onSave });

  return (
    <Card className="py-4">
      <CardContent className="space-y-4">
        <h4 className="text-sm font-medium text-foreground">Edit {project.name || pathKey}</h4>

        <ProjectLogoField
          logoFile={editor.logo.file}
          detectingLogo={editor.logo.detecting}
          detectError={editor.logo.error}
          onDetect={() => void editor.logo.detect()}
        />

        <ProjectEditorFields
          path={project.path || ''}
          name={editor.fields.name}
          nameConflict={editor.validation.nameConflict}
          githubRepo={editor.fields.githubRepo}
          aliases={editor.fields.aliases}
          preamble={editor.fields.preamble}
          scoutSummary={editor.fields.scoutSummary}
          checkCommand={editor.fields.checkCommand}
          onName={editor.fields.setName}
          onGithubRepo={editor.fields.setGithubRepo}
          onAliases={editor.fields.setAliases}
          onPreamble={editor.fields.setPreamble}
          onScoutSummary={editor.fields.setScoutSummary}
          onCheckCommand={editor.fields.setCheckCommand}
        />

        <ProjectHooksFields
          preSpawn={editor.hooks.preSpawn}
          setPreSpawn={editor.hooks.setPreSpawn}
          workerTeardown={editor.hooks.workerTeardown}
          setWorkerTeardown={editor.hooks.setWorkerTeardown}
          postMerge={editor.hooks.postMerge}
          setPostMerge={editor.hooks.setPostMerge}
        />

        <div className="flex items-center gap-3 pt-2">
          <Button
            data-testid="project-save-btn"
            onClick={editor.actions.handleSubmit}
            disabled={!editor.fields.name.trim() || editor.validation.nameConflict || saving}
          >
            {saving ? 'Saving...' : 'Save'}
          </Button>
          <Button variant="ghost" onClick={onCancel}>
            Cancel
          </Button>
        </div>
      </CardContent>
    </Card>
  );
}
