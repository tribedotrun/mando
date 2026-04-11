import { useCallback, useRef, useState } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import log from '#renderer/logger';
import { askTask, fetchAskHistory } from '#renderer/api';
import { queryKeys } from '#renderer/queryKeys';
import type { AskHistoryEntry } from '#renderer/types';

export interface UseTaskAskResult {
  messages: AskHistoryEntry[];
  pending: boolean;
  ask: (question: string) => Promise<void>;
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

  const { data: serverHistory } = useQuery({
    queryKey: queryKeys.tasks.askHistory(itemId),
    queryFn: () => fetchAskHistory(itemId),
  });

  const messages = serverHistory?.history ?? [];

  const ask = useCallback(
    async (question: string) => {
      // Generate ask_id client-side for new conversations so the ref is
      // always set before the API call. On error the server persists the
      // question + error under this same ID, keeping the conversation
      // grouped even when the CC session fails.
      if (!askIdRef.current) {
        askIdRef.current = crypto.randomUUID();
      }
      setPending(true);
      try {
        const data = await askTask(itemId, question, askIdRef.current);
        askIdRef.current = data.ask_id;
        void queryClient.invalidateQueries({ queryKey: queryKeys.tasks.askHistory(itemId) });
      } catch (err) {
        log.warn('[useTaskAsk] ask failed:', err);
        // Error is persisted server-side; invalidate to show it.
        void queryClient.invalidateQueries({ queryKey: queryKeys.tasks.askHistory(itemId) });
      } finally {
        setPending(false);
      }
    },
    [itemId, queryClient],
  );

  return { messages, pending, ask, askId: askIdRef.current };
}
