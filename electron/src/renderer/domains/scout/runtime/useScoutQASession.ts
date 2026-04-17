import { useCallback, useRef, useState } from 'react';
import type { QAEntry } from '#renderer/global/ui/QAChat';
import { useScoutAsk } from '#renderer/domains/scout/repo/mutations';
import { getErrorMessage } from '#renderer/global/service/utils';

export function useScoutQASession(itemId: number) {
  const [history, setHistory] = useState<QAEntry[]>([]);
  const [suggestions, setSuggestions] = useState<string[]>([]);
  const sessionIdRef = useRef<string | undefined>(undefined);
  const askMut = useScoutAsk();

  const ask = useCallback(
    async (question: string, images?: File[]) => {
      setHistory((prev) => [...prev, { role: 'user', text: question }]);
      setSuggestions([]);
      try {
        const data = await askMut.mutateAsync({
          id: itemId,
          question,
          sessionId: sessionIdRef.current,
          images,
        });
        sessionIdRef.current = data.session_id;
        setHistory((prev) => [...prev, { role: 'assistant', text: data.answer }]);
        if (data.suggested_followups?.length) {
          setSuggestions(data.suggested_followups);
        }
      } catch (err) {
        setHistory((prev) => [
          ...prev,
          { role: 'assistant', text: `Error: ${getErrorMessage(err, 'Failed')}` },
        ]);
      }
    },
    [itemId, askMut],
  );

  return { history, pending: askMut.isPending, suggestions, ask };
}
