import { useCallback, useRef, useState } from 'react';
import type { TaskItem } from '#renderer/global/types';
import { useTextImageDraft } from '#renderer/global/runtime/useTextImageDraft';
import { canReopen, canRework, clamp } from '#renderer/global/service/utils';

export type ReopenReworkIntent = 'reopen' | 'rework';

interface Args {
  item: TaskItem;
  onSend: (message: string, intent: ReopenReworkIntent) => void;
  isPending: boolean;
}

export function useReopenReworkComposer({ item, onSend, isPending }: Args) {
  const {
    text: input,
    setText: setInput,
    clearDraft,
  } = useTextImageDraft(`reopen-rework:${item.id}`);
  const reopenAvailable = canReopen(item);
  const reworkAvailable = canRework(item);
  // Fixed initial value: availability at mount time (e.g. while the task is
  // still in-progress and the composer is hidden) must not pick the default.
  const [selectedIntent, setSelectedIntent] = useState<ReopenReworkIntent>('reopen');
  const intent =
    selectedIntent === 'reopen' && reopenAvailable
      ? 'reopen'
      : selectedIntent === 'rework' && reworkAvailable
        ? 'rework'
        : reopenAvailable
          ? 'reopen'
          : 'rework';
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  const handleSubmit = useCallback(() => {
    const trimmed = input.trim();
    if (!trimmed || isPending || (!reopenAvailable && !reworkAvailable)) return;
    onSend(trimmed, intent);
    clearDraft();
    if (textareaRef.current) textareaRef.current.style.height = 'auto';
  }, [clearDraft, input, intent, isPending, onSend, reopenAvailable, reworkAvailable]);

  const handleKeyDown = useCallback(
    (event: React.KeyboardEvent) => {
      if (event.key === 'Enter' && (event.metaKey || event.ctrlKey)) {
        event.preventDefault();
        handleSubmit();
      }
    },
    [handleSubmit],
  );

  const handleInput = useCallback(
    (event: React.ChangeEvent<HTMLTextAreaElement>) => {
      setInput(event.target.value);
      const element = event.target;
      element.style.height = 'auto';
      element.style.height = `${clamp(element.scrollHeight, 56, 256)}px`;
    },
    [setInput],
  );

  return {
    text: { input, textareaRef, handleInput },
    events: { handleSubmit, handleKeyDown },
    intent: {
      value: intent,
      set: setSelectedIntent,
      canReopen: reopenAvailable,
      canRework: reworkAvailable,
    },
    canSubmit: input.trim().length > 0 && !isPending && (reopenAvailable || reworkAvailable),
  };
}
