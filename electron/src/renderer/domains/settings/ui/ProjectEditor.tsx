import React, { useState } from 'react';
import { Card, CardContent } from '#renderer/global/ui/card';
import { Input } from '#renderer/global/ui/input';
import { Textarea } from '#renderer/global/ui/textarea';
import { Label } from '#renderer/global/ui/label';
import { Button } from '#renderer/global/ui/button';
import { ProjectHooksFields } from '#renderer/domains/settings/ui/ProjectHooksFields';
import { ProjectLogoField } from '#renderer/domains/settings/ui/ProjectLogoField';
import type { ProjectConfig } from '#renderer/global/types';
import { useProjectLogo } from '#renderer/domains/settings/runtime/useProjectLogo';

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
    detecting: detectingLogo,
    error: detectError,
    detectLogo,
  } = useProjectLogo(project.name, project.logo || null);
  const [name, setName] = useState(project.name || '');
  const [githubRepo, setGithubRepo] = useState(project.githubRepo || '');
  const [aliases, setAliases] = useState((project.aliases || []).join(', '));
  const [preamble, setPreamble] = useState(project.workerPreamble || '');
  const [checkCommand, setCheckCommand] = useState(project.checkCommand || '');
  const [scoutSummary, setScoutSummary] = useState(project.scoutSummary || '');
  const [preSpawn, setPreSpawn] = useState(project.hooks?.pre_spawn || '');
  const [workerTeardown, setWorkerTeardown] = useState(project.hooks?.worker_teardown || '');
  const [postMerge, setPostMerge] = useState(project.hooks?.post_merge || '');

  const nameLower = name.trim().toLowerCase();
  const nameConflict =
    nameLower.length > 0 &&
    Object.entries(existingProjects).some(
      ([key, value]) => key !== pathKey && value.name?.toLowerCase() === nameLower,
    );

  const handleSubmit = () => {
    if (!name.trim() || nameConflict) return;

    const hooks: Record<string, string> = {};
    if (preSpawn.trim()) hooks.pre_spawn = preSpawn.trim();
    if (workerTeardown.trim()) hooks.worker_teardown = workerTeardown.trim();
    if (postMerge.trim()) hooks.post_merge = postMerge.trim();

    const updated: ProjectConfig = {
      name: name.trim(),
      path: project.path,
      githubRepo: githubRepo.trim() || undefined,
      logo: logoFile ?? undefined,
      aliases: aliases
        .split(',')
        .map((alias) => alias.trim())
        .filter(Boolean),
      workerPreamble: preamble.trim() || undefined,
      checkCommand: checkCommand.trim() || undefined,
      scoutSummary: scoutSummary.trim() || undefined,
      hooks: Object.keys(hooks).length > 0 ? hooks : undefined,
    };
    onSave(pathKey, updated);
  };

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

        <div>
          <Label className="mb-1.5 text-xs text-muted-foreground">Local Path (read-only)</Label>
          <Input
            data-testid="project-path-input"
            value={project.path || ''}
            disabled
            className="opacity-60"
          />
        </div>

        <div>
          <Label className="mb-1.5 text-xs text-muted-foreground">Name</Label>
          <Input
            data-testid="project-name-input"
            value={name}
            onChange={(event) => setName(event.target.value)}
            placeholder="mando"
            aria-invalid={nameConflict || undefined}
          />
          {nameConflict && (
            <p className="mt-1 text-xs text-destructive">
              A project with this name already exists.
            </p>
          )}
        </div>

        <div>
          <Label className="mb-1.5 text-xs text-muted-foreground">
            GitHub Repo (auto-detected, optional)
          </Label>
          <Input
            data-testid="project-github-repo-input"
            value={githubRepo}
            onChange={(event) => setGithubRepo(event.target.value)}
            placeholder="owner/repo"
          />
        </div>

        <div>
          <Label className="mb-1.5 text-xs text-muted-foreground">Aliases (comma-separated)</Label>
          <Input
            value={aliases}
            onChange={(event) => setAliases(event.target.value)}
            placeholder="mdo, mnd"
          />
        </div>

        <div>
          <Label className="mb-1.5 text-xs text-muted-foreground">Worker Preamble</Label>
          <Textarea
            data-testid="project-preamble-input"
            className="h-20 resize-none"
            value={preamble}
            onChange={(event) => setPreamble(event.target.value)}
            placeholder="Instructions prepended to worker prompts..."
          />
        </div>

        <div>
          <Label className="mb-1.5 text-xs text-muted-foreground">Scout Summary</Label>
          <Input
            value={scoutSummary}
            onChange={(event) => setScoutSummary(event.target.value)}
            placeholder="Auto-generated from project metadata"
          />
          <p className="mt-1 text-xs text-muted-foreground">
            Describes this project to Scout for context-aware analysis.
          </p>
        </div>

        <div>
          <Label className="mb-1.5 text-xs text-muted-foreground">Check Command</Label>
          <Input
            data-testid="project-check-command-input"
            value={checkCommand}
            onChange={(event) => setCheckCommand(event.target.value)}
            placeholder="mando-dev check"
          />
          <p className="mt-1 text-xs text-muted-foreground">
            Custom quality-gate command run by captain before marking work complete.
          </p>
        </div>

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
