import { useState } from 'react';
import {
  useScoutItem,
  useScoutArticle,
  useScoutPublishTelegraph,
} from '#renderer/domains/scout/runtime/hooks';
import { openExternalUrl } from '#renderer/global/providers/native/shell';
import { getErrorMessage } from '#renderer/global/service/utils';
import log from '#renderer/global/service/logger';
import { scoutItemTitle } from '#renderer/domains/scout/service/researchHelpers';
import type { ScoutItem } from '#renderer/global/types';

interface UseScoutReaderOptions {
  itemId: number;
}

export interface ScoutReaderState {
  item: {
    value: ScoutItem | null;
    loading: boolean;
    error: string | null;
    displayTitle: string;
  };
  article: {
    body: string | null;
    loading: boolean;
  };
  act: {
    open: boolean;
    setOpen: (v: boolean) => void;
  };
  summary: {
    open: boolean;
    setOpen: (v: boolean) => void;
  };
  publish: {
    telegraphPending: boolean;
    handleTelegraph: () => void;
  };
}

export function useScoutReader({ itemId }: UseScoutReaderOptions): ScoutReaderState {
  const [summaryOpen, setSummaryOpen] = useState(true);
  const [actOpen, setActOpen] = useState(false);
  const publishTelegraphMut = useScoutPublishTelegraph();

  const itemQuery = useScoutItem(itemId);
  const articleQuery = useScoutArticle(itemId);

  const item: ScoutItem | null = itemQuery.data ?? null;
  const loading = itemQuery.isLoading;
  const error = itemQuery.error ? getErrorMessage(itemQuery.error, 'Failed') : null;
  const displayTitle = item ? scoutItemTitle(item) : 'Untitled';
  const article = articleQuery.data?.article ?? null;
  const articleLoading = articleQuery.isLoading;

  const handlePublishTelegraph = () => {
    const openPublishedUrl = async (url: string): Promise<void> => {
      try {
        await openExternalUrl(url);
      } catch (err: unknown) {
        log.warn('[ScoutReader] openExternalUrl failed after Telegraph publish', {
          itemId,
          url,
          err: err instanceof Error ? err.message : String(err),
        });
      }
    };

    publishTelegraphMut.mutate(
      { id: itemId },
      {
        onSuccess: ({ url }) => {
          void openPublishedUrl(url);
        },
      },
    );
  };

  return {
    item: { value: item, loading, error, displayTitle },
    article: { body: article, loading: articleLoading },
    act: { open: actOpen, setOpen: setActOpen },
    summary: { open: summaryOpen, setOpen: setSummaryOpen },
    publish: {
      telegraphPending: publishTelegraphMut.isPending,
      handleTelegraph: handlePublishTelegraph,
    },
  };
}
