import { useRef, useState } from 'react';
import { useRouterState } from '@tanstack/react-router';
import { useTextImageDraft } from '#renderer/global/runtime/useTextImageDraft';
import { useTaskFormPersistence } from '#renderer/domains/captain/runtime/useTaskFormPersistence';
import { useProjects } from '#renderer/global/runtime/useProjects';
import { useConfig } from '#renderer/global/repo/queries';
import { useTaskCreate, useTaskBulkCreate } from '#renderer/domains/captain/runtime/hooks';
import { resolveEffectiveProject } from '#renderer/domains/captain/service/projectHelpers';
import { bulkTextareaRows } from '#renderer/global/service/utils';
import { extractImageFromClipboard } from '#renderer/global/service/clipboardImage';

export function useInlineTaskCreate() {
  const initialProject = useRouterState({
    select: (s) => (s.location.search as { project?: string }).project ?? null,
  });
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
    bulk,
    setBulk,
    project,
    setProject: handleProjectChange,
    resetDrafts,
    persistProject,
  } = useTaskFormPersistence({
    draftProjectKey: 'mando:draft:inlineTask:project',
    draftBulkKey: 'mando:draft:inlineTask:bulk',
    hasDraft,
    initialProject,
  });
  const [noAutoMerge, setNoAutoMerge] = useState(false);
  const [planning, setPlanning] = useState(false);

  const inputRef = useRef<HTMLTextAreaElement>(null);
  const createMut = useTaskCreate();
  const bulkCreateMut = useTaskBulkCreate();
  const projects = useProjects();
  const { data: config } = useConfig();
  const globalAutoMerge = config?.captain?.autoMerge ?? false;

  const { effectiveProject, projectRequired } = resolveEffectiveProject(project, projects);
  const trimmedTitle = title.trim();
  const textareaRows = bulk ? bulkTextareaRows(title.split('\n').length + 1) : 3;
  const pending = createMut.isPending || bulkCreateMut.isPending;

  const resetForm = () => {
    clearDraft();
    resetDrafts();
    setNoAutoMerge(false);
    setPlanning(false);
  };

  const canSubmit = !!trimmedTitle && (!projectRequired || !!effectiveProject) && !pending;

  const handleSubmit = async () => {
    if (!canSubmit) return;
    persistProject(effectiveProject);
    try {
      if (bulk) {
        await bulkCreateMut.mutateAsync({ text: trimmedTitle, project: effectiveProject });
      } else {
        await createMut.mutateAsync({
          title: trimmedTitle,
          project: effectiveProject || undefined,
          noAutoMerge: (globalAutoMerge && noAutoMerge) || undefined,
          planning: planning || undefined,
          images: image ? [image] : undefined,
        });
      }
      resetForm();
    } catch {
      // Mutation hooks surface errors via React Query's `error` state and
      // toast layer; the form keeps the draft so the user can retry.
    }
  };

  const handleKeyDown = (event: React.KeyboardEvent) => {
    if (event.metaKey && event.key === 'Enter') {
      event.preventDefault();
      void handleSubmit();
    }
  };

  const handlePaste = (event: React.ClipboardEvent) => {
    if (bulk) return;
    const file = extractImageFromClipboard(event);
    if (file) setImageFile(file);
  };

  return {
    draft: { title, setTitle, bulk, setBulk, textareaRows, inputRef },
    image: { image, preview, setImageFile, removeImage },
    autoMerge: { globalAutoMerge, noAutoMerge, setNoAutoMerge },
    planMode: { planning, setPlanning },
    project: { projects, effectiveProject, projectRequired, handleProjectChange },
    submit: { pending, canSubmit, handleSubmit },
    events: { handleKeyDown, handlePaste },
  };
}
