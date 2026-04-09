import { useCallback, useRef, useState } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import log from '#renderer/logger';
import { askTask, fetchAskHistory } from '#renderer/api';
import { queryKeys } from '#renderer/queryKeys';
import type { AskHistoryEntry } from '#renderer/types';
import { getErrorMessage } from '#renderer/utils';

export interface UseTaskAskResult {
  messages: AskHistoryEntry[];
  pending: boolean;
  ask: (question: string) => Promise<void>;
}

/**
 * Optimistic ask-task pattern. Appends the human question immediately,
 * awaits the assistant response, and invalidates the server history cache
 * once settled. Clears the optimistic buffer once the server catches up.
 */
export function useTaskAsk(itemId: number): UseTaskAskResult {
  const [localMessages, setLocalMessages] = useState<AskHistoryEntry[]>([]);
  const [pending, setPending] = useState(false);
  const queryClient = useQueryClient();

  const { data: serverHistory } = useQuery({
    queryKey: queryKeys.tasks.askHistory(itemId),
    queryFn: () => fetchAskHistory(itemId),
  });

  const serverMessages = serverHistory?.history ?? [];
  const serverMessagesRef = useRef(serverMessages);
  serverMessagesRef.current = serverMessages;

  // Clear optimistic local cache once server catches up.
  if (localMessages.length > 0 && serverMessages.length >= localMessages.length) {
    setLocalMessages([]);
  }
  const messages = localMessages.length > 0 ? localMessages : serverMessages;

  const ask = useCallback(
    async (question: string) => {
      const now = new Date().toISOString();
      const userEntry: AskHistoryEntry = { role: 'human', content: question, timestamp: now };
      setLocalMessages((prev) => {
        const base = prev.length > 0 ? prev : serverMessagesRef.current;
        return [...base, userEntry];
      });
      setPending(true);
      try {
        const data = await askTask(itemId, question);
        setLocalMessages((prev) => [
          ...prev,
          { role: 'assistant', content: data.answer, timestamp: new Date().toISOString() },
        ]);
        void queryClient.invalidateQueries({ queryKey: queryKeys.tasks.askHistory(itemId) });
      } catch (err) {
        log.warn('[useTaskAsk] ask failed:', err);
        setLocalMessages((prev) => [
          ...prev,
          {
            role: 'assistant',
            content: `Error: ${getErrorMessage(err, 'Failed to get answer')}`,
            timestamp: new Date().toISOString(),
          },
        ]);
      } finally {
        setPending(false);
      }
    },
    [itemId, queryClient],
  );

  return { messages, pending, ask };
}
