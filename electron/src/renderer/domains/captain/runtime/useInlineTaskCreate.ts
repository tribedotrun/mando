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
import {
  applyPlanningProviderChange,
  displayedProvider,
  loadedDefaultProvider,
  providerSelectionReady,
  submittedProvider,
  TASK_PROVIDER_PLANNING,
  type PremiumTaskProvider,
} from '#renderer/domains/captain/runtime/useInlineTaskCreate.helpers';

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
  const [selectedProvider, setSelectedProvider] = useState<PremiumTaskProvider | null>(null);
  const [prePlanningSelectedProvider, setPrePlanningSelectedProvider] =
    useState<PremiumTaskProvider | null>(null);
  // `null` means "follow the configured default"; a boolean is an explicit
  // per-task override the user set via the composer toggle.
  const [glmOverride, setGlmOverride] = useState<boolean | null>(null);

  const handleProviderChange = (next: PremiumTaskProvider) => {
    if (planning && next !== TASK_PROVIDER_PLANNING) return;
    setSelectedProvider(next);
  };

  const handlePlanningChange = (nextPlanning: boolean) => {
    const result = applyPlanningProviderChange({
      nextPlanning,
      wasPlanning: planning,
      selectedProvider,
      prePlanningSelectedProvider,
    });
    setPlanning(nextPlanning);
    setSelectedProvider(result.selectedProvider);
    setPrePlanningSelectedProvider(result.prePlanningSelectedProvider);
  };

  const inputRef = useRef<HTMLTextAreaElement>(null);
  const createMut = useTaskCreate();
  const bulkCreateMut = useTaskBulkCreate();
  const projects = useProjects();
  const { data: config, isSuccess: configLoaded } = useConfig();
  const globalAutoMerge = config?.captain?.autoMerge ?? false;
  // `configGlm` is undefined until config loads. The displayed toggle and the
  // "non-default" baseline use a definite boolean; the submitted value stays
  // undefined when neither an override nor a loaded default is known, so the
  // request omits `use_glm_worker` and the daemon resolves the configured
  // default instead of an early submit forcing GLM off.
  const configGlm = config?.captain?.defaultGlmImplementation;
  const defaultGlmWorker = configGlm ?? false;
  const useGlmWorker = glmOverride ?? defaultGlmWorker;
  const glmForSubmit = glmOverride ?? configGlm;
  const defaultProvider = loadedDefaultProvider(config?.captain?.defaultTaskAgent, configLoaded);
  const provider = displayedProvider(selectedProvider, defaultProvider);
  const providerForSubmit = submittedProvider(selectedProvider, defaultProvider);
  const providerReady = providerSelectionReady(selectedProvider, configLoaded);

  const { effectiveProject, projectRequired } = resolveEffectiveProject(project, projects);
  const trimmedTitle = title.trim();
  const textareaRows = bulk ? bulkTextareaRows(title.split('\n').length + 1) : 3;
  const pending = createMut.isPending || bulkCreateMut.isPending;

  const resetForm = () => {
    clearDraft();
    resetDrafts();
    setNoAutoMerge(false);
    setPlanning(false);
    setGlmOverride(null);
    setSelectedProvider(null);
    setPrePlanningSelectedProvider(null);
  };

  const canSubmit =
    !!trimmedTitle &&
    (!projectRequired || !!effectiveProject) &&
    !pending &&
    (bulk || providerReady);

  const handleSubmit = async () => {
    if (!canSubmit) return;
    persistProject(effectiveProject);
    try {
      if (bulk) {
        await bulkCreateMut.mutateAsync({
          text: trimmedTitle,
          project: effectiveProject,
          useGlmWorker: glmForSubmit,
        });
      } else {
        await createMut.mutateAsync({
          title: trimmedTitle,
          project: effectiveProject || undefined,
          noAutoMerge: (globalAutoMerge && noAutoMerge) || undefined,
          planning: planning || undefined,
          provider: providerForSubmit,
          useGlmWorker: glmForSubmit,
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
    routing: {
      planning,
      setPlanning: handlePlanningChange,
      provider,
      setProvider: handleProviderChange,
      useGlmWorker,
      defaultGlmWorker,
      setUseGlmWorker: setGlmOverride,
    },
    project: { projects, effectiveProject, projectRequired, handleProjectChange },
    submit: { pending, canSubmit, handleSubmit },
    events: { handleKeyDown, handlePaste },
  };
}
