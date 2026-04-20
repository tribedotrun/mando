import React from 'react';
import { Card, CardContent } from '#renderer/global/ui/card';
import { Button } from '#renderer/global/ui/button';
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
  const {
    logoFile,
    detectingLogo,
    detectError,
    detectLogo,
    name,
    setName,
    githubRepo,
    setGithubRepo,
    aliases,
    setAliases,
    preamble,
    setPreamble,
    checkCommand,
    setCheckCommand,
    scoutSummary,
    setScoutSummary,
    preSpawn,
    setPreSpawn,
    workerTeardown,
    setWorkerTeardown,
    postMerge,
    setPostMerge,
    nameConflict,
    handleSubmit,
  } = useProjectEditor({ pathKey, project, existingProjects, onSave });

  return (
    <Card className="py-4">
      <CardContent className="space-y-4">
        <h4 className="text-sm font-medium text-foreground">Edit {project.name || pathKey}</h4>

        <ProjectLogoField
          logoFile={logoFile}
          detectingLogo={detectingLogo}
          detectError={detectError}
          onDetect={() => void detectLogo()}
        />

        <ProjectEditorFields
          path={project.path || ''}
          name={name}
          nameConflict={nameConflict}
          githubRepo={githubRepo}
          aliases={aliases}
          preamble={preamble}
          scoutSummary={scoutSummary}
          checkCommand={checkCommand}
          onName={setName}
          onGithubRepo={setGithubRepo}
          onAliases={setAliases}
          onPreamble={setPreamble}
          onScoutSummary={setScoutSummary}
          onCheckCommand={setCheckCommand}
        />

        <ProjectHooksFields
          preSpawn={preSpawn}
          setPreSpawn={setPreSpawn}
          workerTeardown={workerTeardown}
          setWorkerTeardown={setWorkerTeardown}
          postMerge={postMerge}
          setPostMerge={setPostMerge}
        />

        <div className="flex items-center gap-3 pt-2">
          <Button
            data-testid="project-save-btn"
            onClick={handleSubmit}
            disabled={!name.trim() || nameConflict || saving}
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
