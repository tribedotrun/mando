import React, { useState } from 'react';
import { Card, CardContent } from '#renderer/components/ui/card';
import { Input } from '#renderer/components/ui/input';
import { Textarea } from '#renderer/components/ui/textarea';
import { Label } from '#renderer/components/ui/label';
import { Button } from '#renderer/components/ui/button';
import {
  Collapsible,
  CollapsibleTrigger,
  CollapsibleContent,
} from '#renderer/components/ui/collapsible';
import {
  useSettingsStore,
  type ProjectConfig,
} from '#renderer/domains/settings/stores/settingsStore';
import { shortRepo } from '#renderer/utils';
import { apiPatch, buildUrl } from '#renderer/domains/settings/hooks/useApi';

export interface ProjectEditorProps {
  pathKey: string;
  project: ProjectConfig;
  existingProjects: Record<string, ProjectConfig>;
  onSave: (pathKey: string, project: ProjectConfig) => void;
  onCancel: () => void;
  isNew?: boolean;
}

export function ProjectEditor({
  pathKey: initialPathKey,
  project,
  existingProjects,
  onSave,
  onCancel,
  isNew,
}: ProjectEditorProps): React.ReactElement {
  const reloadConfig = useSettingsStore((s) => s.load);
  const [name, setName] = useState(project.name || '');
  const [logoFile, setLogoFile] = useState(project.logo || null);
  const [detectingLogo, setDetectingLogo] = useState(false);
  const [detectError, setDetectError] = useState<string | null>(null);
  const [projectPath, setProjectPath] = useState(project.path || '');
  const [githubRepo, setGithubRepo] = useState(project.githubRepo || '');
  const [aliases, setAliases] = useState((project.aliases || []).join(', '));
  const [preamble, setPreamble] = useState(project.workerPreamble || '');
  const [scoutSummary, setScoutSummary] = useState(project.scoutSummary || '');
  const [preSpawn, setPreSpawn] = useState(project.hooks?.pre_spawn || '');
  const [workerTeardown, setWorkerTeardown] = useState(project.hooks?.worker_teardown || '');
  const [postMerge, setPostMerge] = useState(project.hooks?.post_merge || '');

  const handleDetectLogo = async () => {
    setDetectingLogo(true);
    setDetectError(null);
    try {
      const res = await apiPatch<{ logo?: string | null }>(
        `/api/projects/${encodeURIComponent(project.name)}`,
        { redetect_logo: true },
      );
      setLogoFile(res.logo ?? null);
      await reloadConfig();
    } catch (err) {
      setDetectError(err instanceof Error ? err.message : 'Detection failed');
    } finally {
      setDetectingLogo(false);
    }
  };

  // Auto-populate name from path when adding a new project.
  const handlePathChange = (value: string) => {
    setProjectPath(value);
    if (isNew && !name) {
      setName(shortRepo(value));
    }
  };

  // Check name uniqueness across all other projects.
  const nameLower = name.trim().toLowerCase();
  const nameConflict =
    nameLower.length > 0 &&
    Object.entries(existingProjects).some(
      ([k, v]) => k !== initialPathKey && v.name?.toLowerCase() === nameLower,
    );

  const handleSubmit = () => {
    if (!name.trim() || !projectPath.trim() || nameConflict) return;
    const hooks: Record<string, string> = {};
    if (preSpawn.trim()) hooks.pre_spawn = preSpawn.trim();
    if (workerTeardown.trim()) hooks.worker_teardown = workerTeardown.trim();
    if (postMerge.trim()) hooks.post_merge = postMerge.trim();

    const pathKey = isNew ? projectPath.trim() : initialPathKey;
    const updated: ProjectConfig = {
      name: name.trim(),
      path: projectPath.trim(),
      githubRepo: githubRepo.trim() || undefined,
      logo: logoFile ?? undefined,
      aliases: aliases
        .split(',')
        .map((a) => a.trim())
        .filter(Boolean),
      workerPreamble: preamble.trim() || undefined,
      scoutSummary: scoutSummary.trim() || undefined,
      hooks: Object.keys(hooks).length > 0 ? hooks : undefined,
    };
    onSave(pathKey, updated);
  };

  return (
    <Card className="py-4">
      <CardContent className="space-y-4">
        <h4 className="text-sm font-medium text-foreground">
          {isNew ? 'Add Project' : `Edit ${project.name || initialPathKey}`}
        </h4>

        {isNew && (
          <div>
            <Label className="mb-1.5 text-xs text-muted-foreground">Local Path</Label>
            <Input
              data-testid="project-path-input"
              value={projectPath}
              onChange={(e) => handlePathChange(e.target.value)}
              placeholder="/Users/you/projects/repo"
            />
          </div>
        )}

        <div>
          <Label className="mb-1.5 text-xs text-muted-foreground">Name</Label>
          <Input
            data-testid="project-name-input"
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="mando"
            aria-invalid={nameConflict || undefined}
          />
          {nameConflict && (
            <p className="mt-1 text-xs text-destructive">
              A project with this name already exists.
            </p>
          )}
        </div>

        {!isNew && (
          <div>
            <Label className="mb-1.5 text-xs text-muted-foreground">Logo</Label>
            <div className="flex items-center gap-3">
              {logoFile && (
                <img
                  src={buildUrl(`/api/images/${logoFile}`)}
                  alt=""
                  width={24}
                  height={24}
                  className="shrink-0 rounded object-contain"
                  onError={(e) => {
                    (e.target as HTMLImageElement).style.display = 'none';
                  }}
                />
              )}
              <Button
                variant="outline"
                size="xs"
                disabled={detectingLogo}
                onClick={() => void handleDetectLogo()}
              >
                {detectingLogo ? 'Detecting...' : logoFile ? 'Re-detect' : 'Detect logo'}
              </Button>
              {detectError && <span className="text-xs text-destructive">{detectError}</span>}
              {!logoFile && !detectingLogo && !detectError && (
                <span className="text-xs text-muted-foreground">No logo detected</span>
              )}
            </div>
          </div>
        )}

        {!isNew && (
          <div>
            <Label className="mb-1.5 text-xs text-muted-foreground">Local Path (read-only)</Label>
            <Input
              data-testid="project-path-input"
              value={projectPath}
              disabled
              className="opacity-60"
            />
          </div>
        )}

        <div>
          <Label className="mb-1.5 text-xs text-muted-foreground">
            GitHub Repo (auto-detected, optional)
          </Label>
          <Input
            data-testid="project-github-repo-input"
            value={githubRepo}
            onChange={(e) => setGithubRepo(e.target.value)}
            placeholder="owner/repo"
          />
        </div>

        <div>
          <Label className="mb-1.5 text-xs text-muted-foreground">Aliases (comma-separated)</Label>
          <Input
            value={aliases}
            onChange={(e) => setAliases(e.target.value)}
            placeholder="mdo, mnd"
          />
        </div>

        <div>
          <Label className="mb-1.5 text-xs text-muted-foreground">Worker Preamble</Label>
          <Textarea
            data-testid="project-preamble-input"
            className="h-20 resize-none"
            value={preamble}
            onChange={(e) => setPreamble(e.target.value)}
            placeholder="Instructions prepended to worker prompts..."
          />
        </div>

        <div>
          <Label className="mb-1.5 text-xs text-muted-foreground">Scout Summary</Label>
          <Input
            value={scoutSummary}
            onChange={(e) => setScoutSummary(e.target.value)}
            placeholder="Auto-generated from project metadata"
          />
          <p className="mt-1 text-xs text-muted-foreground">
            Describes this project to Scout for context-aware analysis.
          </p>
        </div>

        {/* Hooks */}
        <Collapsible className="group">
          <CollapsibleTrigger className="cursor-pointer text-xs font-medium text-muted-foreground">
            Hooks (optional)
          </CollapsibleTrigger>
          <CollapsibleContent className="mt-3 space-y-3">
            <div>
              <Label className="mb-1.5 text-xs text-muted-foreground">pre_spawn</Label>
              <Input
                value={preSpawn}
                onChange={(e) => setPreSpawn(e.target.value)}
                placeholder="path/to/script.sh"
              />
            </div>
            <div>
              <Label className="mb-1.5 text-xs text-muted-foreground">worker_teardown</Label>
              <Input
                value={workerTeardown}
                onChange={(e) => setWorkerTeardown(e.target.value)}
                placeholder="path/to/script.sh"
              />
            </div>
            <div>
              <Label className="mb-1.5 text-xs text-muted-foreground">post_merge</Label>
              <Input
                value={postMerge}
                onChange={(e) => setPostMerge(e.target.value)}
                placeholder="path/to/script.sh"
              />
            </div>
          </CollapsibleContent>
        </Collapsible>

        <div className="flex items-center gap-3 pt-2">
          <Button
            data-testid="project-save-btn"
            onClick={handleSubmit}
            disabled={!name.trim() || !projectPath.trim() || nameConflict}
          >
            {isNew ? 'Add' : 'Save'}
          </Button>
          <Button variant="ghost" onClick={onCancel}>
            Cancel
          </Button>
        </div>
      </CardContent>
    </Card>
  );
}
