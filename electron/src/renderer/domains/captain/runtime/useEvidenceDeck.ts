import { useMemo } from 'react';
import {
  useEvidenceDeckAvailability,
  useEvidenceDeckSource,
  useTaskArtifacts,
} from '#renderer/domains/captain/repo/queries';
import { prepareEvidenceDeck } from '#renderer/domains/captain/service/evidenceDeck';
import type { TaskItem } from '#renderer/global/types';

export function useEvidenceDeck(item: TaskItem, activeTab: string | undefined) {
  const { dataUpdatedAt } = useTaskArtifacts(item.id);
  const worktree = item.worktree;
  const version = `${worktree ?? 'none'}:${item.rev}:${dataUpdatedAt}`;
  const deckIsActive = activeTab === 'deck';
  const availability = useEvidenceDeckAvailability(item.id, version, worktree);
  const fileAvailable = availability.data === true;
  const deck = useEvidenceDeckSource(item.id, version, worktree, fileAvailable && deckIsActive);
  const prepared = useMemo(() => (deck.data ? prepareEvidenceDeck(deck.data) : null), [deck.data]);
  const loadFailed = deckIsActive && deck.isSuccess && deck.data === null;
  const available = fileAvailable && !loadFailed;
  const document = deckIsActive && !deck.isFetching ? prepared : null;

  return { available, document, pending: deckIsActive && deck.isFetching };
}
