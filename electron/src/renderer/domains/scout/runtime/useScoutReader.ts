import { useState } from 'react';
import {
  useScoutItem,
  useScoutArticle,
  useScoutAct,
  useScoutPublishTelegraph,
} from '#renderer/domains/scout/runtime/hooks';
import { useProjects } from '#renderer/global/runtime/useProjects';
import { openExternalUrl } from '#renderer/global/providers/native/shell';
import { getErrorMessage } from '#renderer/global/service/utils';
import log from '#renderer/global/service/logger';
import { scoutItemTitle, formatActResult } from '#renderer/domains/scout/service/researchHelpers';
import type { ScoutItem } from '#renderer/global/types';

interface UseScoutReaderOptions {
  itemId: number;
}

export interface ScoutReaderState {
  item: ScoutItem | null;
  loading: boolean;
  error: string | null;
  displayTitle: string;
  article: string | null;
  articleLoading: boolean;
  projects: string[];
  actOpen: boolean;
  setActOpen: (v: boolean) => void;
  summaryOpen: boolean;
  setSummaryOpen: (v: boolean) => void;
  actProject: string;
  setActProject: (v: string) => void;
  effectiveActProject: string;
  actPrompt: string;
  setActPrompt: (v: string) => void;
  acting: boolean;
  actResult: ReturnType<typeof formatActResult>;
  resetAct: () => void;
  handleAct: () => void;
  publishingTelegraph: boolean;
  handlePublishTelegraph: () => void;
}

export function useScoutReader({ itemId }: UseScoutReaderOptions): ScoutReaderState {
  const [summaryOpen, setSummaryOpen] = useState(true);
  const [actOpen, setActOpen] = useState(false);
  const [actProject, setActProject] = useState('');
  const [actPrompt, setActPrompt] = useState('');
  const actMut = useScoutAct();
  const publishTelegraphMut = useScoutPublishTelegraph();

  const projects = useProjects();

  // Derive effective project: auto-select when exactly one exists
  const effectiveActProject = actProject || (projects.length === 1 ? projects[0] : '');

  const itemQuery = useScoutItem(itemId);
  const articleQuery = useScoutArticle(itemId);

  const item: ScoutItem | null = itemQuery.data ?? null;
  const loading = itemQuery.isLoading;
  const error = itemQuery.error ? getErrorMessage(itemQuery.error, 'Failed') : null;
  const displayTitle = item ? scoutItemTitle(item) : 'Untitled';
  const article = articleQuery.data?.article ?? null;
  const articleLoading = articleQuery.isLoading;

  const actResult = formatActResult(actMut.data, actMut.error);

  const handleAct = () => {
    if (!effectiveActProject) return;
    actMut.reset();
    actMut.mutate(
      { id: itemId, project: effectiveActProject, prompt: actPrompt || undefined },
      {
        onError: (e) => log.warn('[ScoutReader] actOnScoutItem failed', { itemId, err: e }),
      },
    );
  };

  const handlePublishTelegraph = () => {
    publishTelegraphMut.mutate(
      { id: itemId },
      {
        onSuccess: ({ url }) => {
          void openExternalUrl(url).catch((err: unknown) => {
            log.warn('[ScoutReader] openExternalUrl failed after Telegraph publish', {
              itemId,
              url,
              err: err instanceof Error ? err.message : String(err),
            });
          });
        },
      },
    );
  };

  return {
    item,
    loading,
    error,
    displayTitle,
    article,
    articleLoading,
    projects,
    actOpen,
    setActOpen,
    summaryOpen,
    setSummaryOpen,
    actProject,
    setActProject,
    effectiveActProject,
    actPrompt,
    setActPrompt,
    acting: actMut.isPending,
    actResult,
    resetAct: actMut.reset,
    handleAct,
    publishingTelegraph: publishTelegraphMut.isPending,
    handlePublishTelegraph,
  };
}
