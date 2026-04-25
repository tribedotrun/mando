import { useRef, useState } from 'react';
import { useTextImageDraft } from '#renderer/global/runtime/useTextImageDraft';
import { useTaskFormPersistence } from '#renderer/domains/captain/runtime/useTaskFormPersistence';
import { useProjects } from '#renderer/global/runtime/useProjects';
import { useConfig } from '#renderer/global/repo/queries';
import { useTaskCreate } from '#renderer/domains/captain/runtime/hooks';
import { resolveEffectiveProject } from '#renderer/domains/captain/service/projectHelpers';
import { extractImageFromClipboard } from '#renderer/global/service/clipboardImage';

export function useInlineTaskCreate() {
  const {
    text: title,
    setText: setTitle,
    image,
    preview,
    setImageFile,
    removeImage,
    clearDraft,
  } = useTextImageDraft('inlineTask', { legacyTextSuffix: 'inlineTask' });
  const hasDraft = title !== '';
  const {
    project,
    setProject: handleProjectChange,
    resetDrafts,
    persistProject,
  } = useTaskFormPersistence({
    draftProjectKey: 'mando:draft:inlineTask:project',
    hasDraft,
  });
  const [noAutoMerge, setNoAutoMerge] = useState(false);
  const [planning, setPlanning] = useState(false);

  const inputRef = useRef<HTMLTextAreaElement>(null);
  const createMut = useTaskCreate();
  const projects = useProjects();
  const { data: config } = useConfig();
  const globalAutoMerge = config?.captain?.autoMerge ?? false;

  const { effectiveProject, projectRequired } = resolveEffectiveProject(project, projects);
  const trimmedTitle = title.trim();

  const resetForm = () => {
    clearDraft();
    resetDrafts();
    setNoAutoMerge(false);
    setPlanning(false);
  };

  const canSubmit =
    !!trimmedTitle && (!projectRequired || !!effectiveProject) && !createMut.isPending;

  const handleSubmit = () => {
    if (!canSubmit) return;
    persistProject(effectiveProject);
    createMut.mutate(
      {
        title: trimmedTitle,
        project: effectiveProject || undefined,
        noAutoMerge: (globalAutoMerge && noAutoMerge) || undefined,
        planning: planning || undefined,
        images: image ? [image] : undefined,
      },
      { onSuccess: () => resetForm() },
    );
  };

  const handleKeyDown = (event: React.KeyboardEvent) => {
    if (event.metaKey && event.key === 'Enter') {
      event.preventDefault();
      handleSubmit();
    }
  };

  const handlePaste = (event: React.ClipboardEvent) => {
    const file = extractImageFromClipboard(event);
    if (file) setImageFile(file);
  };

  return {
    draft: { title, setTitle, inputRef },
    image: { image, preview, setImageFile, removeImage },
    autoMerge: { globalAutoMerge, noAutoMerge, setNoAutoMerge },
    planMode: { planning, setPlanning },
    project: { projects, effectiveProject, projectRequired, handleProjectChange },
    submit: { pending: createMut.isPending, canSubmit, handleSubmit },
    events: { handleKeyDown, handlePaste },
  };
}
