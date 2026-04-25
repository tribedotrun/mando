import React from 'react';
import { Input } from '#renderer/global/ui/primitives/input';
import { Textarea } from '#renderer/global/ui/primitives/textarea';
import { Label } from '#renderer/global/ui/primitives/label';

interface Props {
  path: string;
  name: string;
  nameConflict: boolean;
  githubRepo: string;
  aliases: string;
  preamble: string;
  scoutSummary: string;
  checkCommand: string;
  onName: (v: string) => void;
  onGithubRepo: (v: string) => void;
  onAliases: (v: string) => void;
  onPreamble: (v: string) => void;
  onScoutSummary: (v: string) => void;
  onCheckCommand: (v: string) => void;
}

export function ProjectEditorFields({
  path,
  name,
  nameConflict,
  githubRepo,
  aliases,
  preamble,
  scoutSummary,
  checkCommand,
  onName,
  onGithubRepo,
  onAliases,
  onPreamble,
  onScoutSummary,
  onCheckCommand,
}: Props): React.ReactElement {
  return (
    <>
      <div>
        <Label className="mb-1.5 text-xs text-muted-foreground">Local Path (read-only)</Label>
        <Input data-testid="project-path-input" value={path} disabled className="opacity-60" />
      </div>

      <div>
        <Label className="mb-1.5 text-xs text-muted-foreground">Name</Label>
        <Input
          data-testid="project-name-input"
          value={name}
          onChange={(e) => onName(e.target.value)}
          placeholder="mando"
          aria-invalid={nameConflict || undefined}
        />
        {nameConflict && (
          <p className="mt-1 text-xs text-destructive">A project with this name already exists.</p>
        )}
      </div>

      <div>
        <Label className="mb-1.5 text-xs text-muted-foreground">
          GitHub Repo (auto-detected, optional)
        </Label>
        <Input
          data-testid="project-github-repo-input"
          value={githubRepo}
          onChange={(e) => onGithubRepo(e.target.value)}
          placeholder="owner/repo"
        />
      </div>

      <div>
        <Label className="mb-1.5 text-xs text-muted-foreground">Aliases (comma-separated)</Label>
        <Input value={aliases} onChange={(e) => onAliases(e.target.value)} placeholder="mdo, mnd" />
      </div>

      <div>
        <Label className="mb-1.5 text-xs text-muted-foreground">Worker Preamble</Label>
        <Textarea
          data-testid="project-preamble-input"
          className="h-20 resize-none"
          value={preamble}
          onChange={(e) => onPreamble(e.target.value)}
          placeholder="Instructions prepended to worker prompts..."
        />
      </div>

      <div>
        <Label className="mb-1.5 text-xs text-muted-foreground">Scout Summary</Label>
        <Input
          value={scoutSummary}
          onChange={(e) => onScoutSummary(e.target.value)}
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
          onChange={(e) => onCheckCommand(e.target.value)}
          placeholder="mando-dev check"
        />
        <p className="mt-1 text-xs text-muted-foreground">
          Custom quality-gate command run by captain before marking work complete.
        </p>
      </div>
    </>
  );
}
