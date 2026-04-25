import { useCallback, useRef, useState } from 'react';
import type { TaskItem } from '#renderer/global/types';
import { useTextImageDraft } from '#renderer/global/runtime/useTextImageDraft';
import { canReopen, canRework, canRevisePlan, clamp } from '#renderer/global/service/utils';

export type AdvisorIntent = 'ask' | 'reopen' | 'rework' | 'revise-plan';

interface Args {
  item: TaskItem;
  onSend: (message: string, intent: AdvisorIntent) => void;
  isPending: boolean;
}

export function useAdvisorInputBar({ item, onSend, isPending }: Args) {
  const { text: input, setText: setInput, clearDraft } = useTextImageDraft(`advisor:${item.id}`);
  const [intent, setIntent] = useState<AdvisorIntent>('ask');
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  const handleSubmit = useCallback(() => {
    const trimmed = input.trim();
    if (!trimmed || isPending) return;
    onSend(trimmed, intent);
    clearDraft();
    setIntent('ask');
    if (textareaRef.current) textareaRef.current.style.height = 'auto';
  }, [clearDraft, input, intent, isPending, onSend]);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
        e.preventDefault();
        handleSubmit();
      }
    },
    [handleSubmit],
  );

  const handleInput = useCallback(
    (e: React.ChangeEvent<HTMLTextAreaElement>) => {
      setInput(e.target.value);
      const el = e.target;
      el.style.height = 'auto';
      el.style.height = `${clamp(el.scrollHeight, 56, 256)}px`;
    },
    [setInput],
  );

  return {
    text: { input, textareaRef, handleInput },
    events: { handleSubmit, handleKeyDown },
    intent: {
      value: intent,
      set: setIntent,
      canAsk: true,
      canReopen: canReopen(item),
      canRework: canRework(item),
      canRevise: canRevisePlan(item),
    },
    canSubmit: input.trim().length > 0 && !isPending,
  };
}
