import { useRef, useState } from 'react';
import { useScoutAct } from '#renderer/domains/scout/runtime/hooks';
import { useProjects } from '#renderer/global/runtime/useProjects';
import { formatActResult } from '#renderer/domains/scout/service/researchHelpers';
import { useTextImageDraft } from '#renderer/global/runtime/useTextImageDraft';
import log from '#renderer/global/service/logger';

export function useScoutActForm(itemId: number, open: boolean) {
  const [project, setProject] = useState('');
  const {
    text: prompt,
    setText: setPrompt,
    clearDraft: clearPromptDraft,
  } = useTextImageDraft(`scoutAct:${itemId}`);
  const projects = useProjects();
  const actMut = useScoutAct();
  const prevOpenRef = useRef(open);
  if (prevOpenRef.current !== open) {
    prevOpenRef.current = open;
    actMut.reset();
  }

  const effectiveProject = project || (projects.length === 1 ? projects[0] : '');

  const handleAct = () => {
    if (!effectiveProject) return;
    actMut.reset();
    actMut.mutate(
      { id: itemId, project: effectiveProject, prompt: prompt || undefined },
      {
        onSuccess: () => clearPromptDraft(),
        onError: (err) => log.warn('[ScoutActForm] actOnScoutItem failed', { itemId, err }),
      },
    );
  };

  return {
    projects,
    project: effectiveProject,
    setProject,
    prompt,
    setPrompt,
    pending: actMut.isPending,
    result: formatActResult(actMut.data, actMut.error),
    handleAct,
  };
}
