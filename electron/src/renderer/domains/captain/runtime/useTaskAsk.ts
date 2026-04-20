import { useCallback, useRef, useState } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import log from '#renderer/global/service/logger';
import { askTask } from '#renderer/domains/captain/repo/api';
import { useTaskAskHistory } from '#renderer/domains/captain/repo/queries';
import { queryKeys } from '#renderer/global/repo/queryKeys';
import { toReactQuery } from '#result';
import type { AskHistoryEntry } from '#renderer/global/types';

export interface UseTaskAskResult {
  messages: AskHistoryEntry[];
  pending: boolean;
  ask: (question: string, images?: File[]) => Promise<void>;
  /** Current conversation ID. Undefined until first ask. */
  askId: string | undefined;
}

/**
 * Task Q&A hook. The backend persists the question immediately (before
 * the CC session runs), so SSE invalidation keeps the UI in sync.
 * We generate `askId` client-side so it's available even when the
 * server returns an error (the question + error are still persisted
 * under this ID).
 */
export function useTaskAsk(itemId: number): UseTaskAskResult {
  const [pending, setPending] = useState(false);
  const askIdRef = useRef<string | undefined>(undefined);
  const queryClient = useQueryClient();

  const { data: serverHistory } = useTaskAskHistory(itemId);

  const messages = serverHistory?.history ?? [];

  const ask = useCallback(
    async (question: string, images?: File[]) => {
      if (!askIdRef.current) {
        askIdRef.current = crypto.randomUUID();
      }
      setPending(true);
      try {
        const data = await toReactQuery(askTask(itemId, question, askIdRef.current, images));
        askIdRef.current = data.ask_id;
        void queryClient.invalidateQueries({ queryKey: queryKeys.tasks.askHistory(itemId) });
      } catch (err) {
        log.warn('[useTaskAsk] ask failed:', err);
        void queryClient.invalidateQueries({ queryKey: queryKeys.tasks.askHistory(itemId) });
      } finally {
        setPending(false);
      }
    },
    [itemId, queryClient],
  );

  return { messages, pending, ask, askId: askIdRef.current };
}
