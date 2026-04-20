import { useState } from 'react';
import type { ProjectConfig } from '#renderer/global/types';
import { useProjectLogo } from '#renderer/domains/settings/runtime/useProjectLogo';

interface Args {
  pathKey: string;
  project: ProjectConfig;
  existingProjects: Record<string, ProjectConfig>;
  onSave: (pathKey: string, project: ProjectConfig) => void;
}

export function useProjectEditor({ pathKey, project, existingProjects, onSave }: Args) {
  const [name, setName] = useState(project.name || '');

  // `useProjectLogo` resolves by stored project identity, so it must use the server-side
  // name (`project.name`), not the mutable draft, or detect can miss the current project.
  const {
    logoFile,
    detecting: detectingLogo,
    error: detectError,
    detectLogo,
  } = useProjectLogo(project.name ?? pathKey, project.logo || null);
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

  return {
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
  };
}
