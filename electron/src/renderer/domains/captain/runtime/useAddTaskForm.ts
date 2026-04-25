import { useRef, useState } from 'react';
import { useTextImageDraft } from '#renderer/global/runtime/useTextImageDraft';
import { useTaskFormPersistence } from '#renderer/domains/captain/runtime/useTaskFormPersistence';
import { useMountEffect } from '#renderer/global/runtime/useMountEffect';
import { useProjects } from '#renderer/global/runtime/useProjects';
import { useConfig } from '#renderer/global/repo/queries';
import { useTaskCreate, useTaskBulkCreate } from '#renderer/domains/captain/runtime/hooks';
import { resolveEffectiveProject } from '#renderer/domains/captain/service/projectHelpers';
import { bulkTextareaRows } from '#renderer/global/service/utils';
import { extractImageFromClipboard } from '#renderer/global/service/clipboardImage';

const AUTOFOCUS_DELAY_MS = 50;

interface Args {
  onClose: () => void;
  initialProject?: string | null;
}

export function useAddTaskForm({ onClose, initialProject }: Args) {
  const {
    text: title,
    setText: setTitle,
    image,
    preview,
    setImageFile,
    removeImage,
    clearDraft,
  } = useTextImageDraft('newTask', { legacyTextSuffix: 'newTask' });
  const hasDraft = title !== '';
  const {
    bulk,
    setBulk,
    project,
    setProject: handleProjectChange,
    resetDrafts,
    cleanupIfEmpty,
    persistProject,
  } = useTaskFormPersistence({
    draftProjectKey: 'mando:draft:newTask:project',
    draftBulkKey: 'mando:draft:newTask:bulk',
    hasDraft,
    initialProject,
  });
  const [submitError, setSubmitError] = useState<string | null>(null);
  const [noAutoMerge, setNoAutoMerge] = useState(false);
  const [planning, setPlanning] = useState(false);

  const titleRef = useRef(title);
  titleRef.current = title;
  const inputRef = useRef<HTMLTextAreaElement>(null);
  const createMut = useTaskCreate();
  const bulkCreateMut = useTaskBulkCreate();
  const createPhase = createMut.isPending || bulkCreateMut.isPending ? 'active' : 'idle';

  const projects = useProjects();
  const { data: config } = useConfig();
  const globalAutoMerge = config?.captain?.autoMerge ?? false;

  const { effectiveProject, projectRequired } = resolveEffectiveProject(project, projects);
  const trimmedTitle = title.trim();
  const textareaRows = bulk ? bulkTextareaRows(title.split('\n').length + 1) : 5;

  useMountEffect(() => {
    setTimeout(() => inputRef.current?.focus(), AUTOFOCUS_DELAY_MS);
    return () => cleanupIfEmpty(!titleRef.current.trim());
  });

  const resetForm = () => {
    setBulk(false);
    clearDraft();
    resetDrafts();
    setSubmitError(null);
    setNoAutoMerge(false);
    setPlanning(false);
  };

  const canSubmit =
    !!trimmedTitle && (!projectRequired || !!effectiveProject) && createPhase === 'idle';

  const handleSubmit = () => {
    if (!trimmedTitle) return;
    if (projectRequired && !effectiveProject) {
      setSubmitError('Select a project before handing work to Mando.');
      return;
    }
    persistProject(effectiveProject);
    if (bulk) {
      bulkCreateMut.mutate({ text: trimmedTitle, project: effectiveProject });
    } else {
      createMut.mutate({
        title: trimmedTitle,
        project: effectiveProject || undefined,
        noAutoMerge: (globalAutoMerge && noAutoMerge) || undefined,
        planning: planning || undefined,
        images: image ? [image] : undefined,
      });
    }
    resetForm();
    onClose();
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.metaKey && e.key === 'Enter') {
      e.preventDefault();
      handleSubmit();
    }
    if (e.key === 'Escape') onClose();
  };

  const handlePaste = (e: React.ClipboardEvent) => {
    const file = extractImageFromClipboard(e);
    if (file) setImageFile(file);
  };

  return {
    draft: { title, setTitle, bulk, setBulk, textareaRows, inputRef, submitError },
    project: { projects, effectiveProject, projectRequired, handleProjectChange },
    image: { image, preview, setImageFile, removeImage },
    autoMerge: { globalAutoMerge, noAutoMerge, setNoAutoMerge },
    planMode: { planning, setPlanning },
    submit: { createPhase, canSubmit, handleSubmit },
    events: { handleKeyDown, handlePaste },
  };
}
