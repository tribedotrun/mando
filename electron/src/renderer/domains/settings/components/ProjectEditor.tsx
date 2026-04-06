import React, { useState } from 'react';
import { inputStyle, labelStyle, inputCls, labelCls } from '#renderer/styles';
import type { ProjectConfig } from '#renderer/domains/settings/stores/settingsStore';
import { shortRepo } from '#renderer/utils';

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
  const [name, setName] = useState(project.name || '');
  const [projectPath, setProjectPath] = useState(project.path || '');
  const [githubRepo, setGithubRepo] = useState(project.githubRepo || '');
  const [aliases, setAliases] = useState((project.aliases || []).join(', '));
  const [preamble, setPreamble] = useState(project.workerPreamble || '');
  const [scoutSummary, setScoutSummary] = useState(project.scoutSummary || '');
  const [preSpawn, setPreSpawn] = useState(project.hooks?.pre_spawn || '');
  const [workerTeardown, setWorkerTeardown] = useState(project.hooks?.worker_teardown || '');
  const [postMerge, setPostMerge] = useState(project.hooks?.post_merge || '');

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

  const focusStyle = {
    ...inputStyle,
    '--tw-ring-color': 'var(--color-accent)',
  } as React.CSSProperties;

  return (
    <div
      className="space-y-4 rounded-lg p-5"
      style={{
        border: '1px solid var(--color-border)',
        background: 'var(--color-surface-1)',
      }}
    >
      <h4 className="text-sm font-medium" style={{ color: 'var(--color-text-1)' }}>
        {isNew ? 'Add Project' : `Edit ${project.name || initialPathKey}`}
      </h4>

      {isNew && (
        <div>
          <label className={labelCls} style={labelStyle}>
            Local Path
          </label>
          <input
            data-testid="project-path-input"
            className={inputCls}
            style={focusStyle}
            value={projectPath}
            onChange={(e) => handlePathChange(e.target.value)}
            placeholder="/Users/you/projects/repo"
          />
        </div>
      )}

      <div>
        <label className={labelCls} style={labelStyle}>
          Name
        </label>
        <input
          data-testid="project-name-input"
          className={inputCls}
          style={{
            ...focusStyle,
            ...(nameConflict ? { borderColor: 'var(--color-error)' } : {}),
          }}
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder="mando"
        />
        {nameConflict && (
          <p className="mt-1 text-xs" style={{ color: 'var(--color-error)' }}>
            A project with this name already exists.
          </p>
        )}
      </div>

      {!isNew && (
        <div>
          <label className={labelCls} style={labelStyle}>
            Local Path (read-only)
          </label>
          <input
            data-testid="project-path-input"
            className={`${inputCls} opacity-60`}
            style={focusStyle}
            value={projectPath}
            disabled
          />
        </div>
      )}

      <div>
        <label className={labelCls} style={labelStyle}>
          GitHub Repo (auto-detected, optional)
        </label>
        <input
          data-testid="project-github-repo-input"
          className={inputCls}
          style={focusStyle}
          value={githubRepo}
          onChange={(e) => setGithubRepo(e.target.value)}
          placeholder="owner/repo"
        />
      </div>

      <div>
        <label className={labelCls} style={labelStyle}>
          Aliases (comma-separated)
        </label>
        <input
          className={inputCls}
          style={focusStyle}
          value={aliases}
          onChange={(e) => setAliases(e.target.value)}
          placeholder="mdo, mnd"
        />
      </div>

      <div>
        <label className={labelCls} style={labelStyle}>
          Worker Preamble
        </label>
        <textarea
          data-testid="project-preamble-input"
          className={`${inputCls} h-20 resize-none`}
          style={focusStyle}
          value={preamble}
          onChange={(e) => setPreamble(e.target.value)}
          placeholder="Instructions prepended to worker prompts..."
        />
      </div>

      <div>
        <label className={labelCls} style={labelStyle}>
          Scout Summary
        </label>
        <input
          className={inputCls}
          style={focusStyle}
          value={scoutSummary}
          onChange={(e) => setScoutSummary(e.target.value)}
          placeholder="Auto-generated from project metadata"
        />
        <p className="mt-1 text-xs" style={{ color: 'var(--color-text-3)' }}>
          Describes this project to Scout for context-aware analysis.
        </p>
      </div>

      {/* Hooks */}
      <details className="group">
        <summary
          className="cursor-pointer text-xs font-medium"
          style={{ color: 'var(--color-text-2)' }}
        >
          Hooks (optional)
        </summary>
        <div className="mt-3 space-y-3">
          <div>
            <label className={labelCls} style={labelStyle}>
              pre_spawn
            </label>
            <input
              className={inputCls}
              style={focusStyle}
              value={preSpawn}
              onChange={(e) => setPreSpawn(e.target.value)}
              placeholder="path/to/script.sh"
            />
          </div>
          <div>
            <label className={labelCls} style={labelStyle}>
              worker_teardown
            </label>
            <input
              className={inputCls}
              style={focusStyle}
              value={workerTeardown}
              onChange={(e) => setWorkerTeardown(e.target.value)}
              placeholder="path/to/script.sh"
            />
          </div>
          <div>
            <label className={labelCls} style={labelStyle}>
              post_merge
            </label>
            <input
              className={inputCls}
              style={focusStyle}
              value={postMerge}
              onChange={(e) => setPostMerge(e.target.value)}
              placeholder="path/to/script.sh"
            />
          </div>
        </div>
      </details>

      <div className="flex items-center gap-3 pt-2">
        <button
          data-testid="project-save-btn"
          onClick={handleSubmit}
          disabled={!name.trim() || !projectPath.trim() || nameConflict}
          className="rounded-md px-4 py-2 text-sm font-medium disabled:opacity-40"
          style={{ background: 'var(--color-accent)', color: 'var(--color-bg)' }}
        >
          {isNew ? 'Add' : 'Save'}
        </button>
        <button
          onClick={onCancel}
          className="rounded-md px-4 py-2 text-sm"
          style={{ color: 'var(--color-text-2)' }}
        >
          Cancel
        </button>
      </div>
    </div>
  );
}
