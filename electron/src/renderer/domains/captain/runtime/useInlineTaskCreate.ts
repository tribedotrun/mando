import { useRef, useState } from 'react';
import { useDraft } from '#renderer/domains/captain/runtime/useDraft';
import { useImageAttachment } from '#renderer/global/runtime/useImageAttachment';
import { useTaskFormPersistence } from '#renderer/domains/captain/runtime/useTaskFormPersistence';
import { useProjects } from '#renderer/global/runtime/useProjects';
import { useConfig } from '#renderer/global/repo/queries';
import { useTaskCreate } from '#renderer/domains/captain/runtime/hooks';
import { resolveEffectiveProject } from '#renderer/domains/captain/service/projectHelpers';
import { extractImageFromClipboard } from '#renderer/global/service/clipboardImage';

export function useInlineTaskCreate() {
  const [title, setTitle, clearTitleDraft] = useDraft('mando:draft:inlineTask');
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
  const { image, preview, setImageFile, removeImage } = useImageAttachment();
  const [noAutoMerge, setNoAutoMerge] = useState(false);

  const inputRef = useRef<HTMLTextAreaElement>(null);
  const createMut = useTaskCreate();
  const projects = useProjects();
  const { data: config } = useConfig();
  const globalAutoMerge = config?.captain?.autoMerge ?? false;

  const { effectiveProject, projectRequired } = resolveEffectiveProject(project, projects);
  const trimmedTitle = title.trim();

  const resetForm = () => {
    clearTitleDraft();
    resetDrafts();
    setNoAutoMerge(false);
    removeImage();
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
    title,
    setTitle,
    image,
    preview,
    setImageFile,
    removeImage,
    noAutoMerge,
    setNoAutoMerge,
    inputRef,
    createMut,
    projects,
    globalAutoMerge,
    effectiveProject,
    projectRequired,
    canSubmit,
    handleSubmit,
    handleKeyDown,
    handlePaste,
    handleProjectChange,
  };
}
