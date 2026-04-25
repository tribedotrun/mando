import React, { useState, useCallback, useRef } from 'react';
import {
  useTaskAskHistory,
  useTaskAskReopen,
  useTaskReopen,
  useTaskRework,
} from '#renderer/domains/captain/runtime/hooks';
import {
  type ActionBarAction,
  ACTION_CONFIG,
  getAvailableActions,
  getDefaultAction,
  isActionBarHidden,
  shouldShowAskReopen,
} from '#renderer/domains/captain/service/actionBarHelpers';
import { useTextImageDraft } from '#renderer/global/runtime/useTextImageDraft';
import { extractImageFromClipboard } from '#renderer/global/service/clipboardImage';
import type { TaskItem } from '#renderer/global/types';
import { Textarea } from '#renderer/global/ui/primitives/textarea';
import { ActionBarFooter, ImageChip } from '#renderer/domains/captain/ui/ActionBarFooter';

interface Props {
  item: TaskItem;
  onAsk?: (question: string, images?: File[]) => void;
}

export function TaskComposer({ item, onAsk }: Props): React.ReactElement | null {
  const available = getAvailableActions(item);
  const defaultAction = getDefaultAction(item);
  const [selectedAction, setSelectedAction] = useState<ActionBarAction>(defaultAction);
  const { text, setText, image, preview, setImageFile, removeImage, clearDraft } =
    useTextImageDraft(`action:${item.id}`, { legacyTextSuffix: `action:${item.id}` });
  const [pendingAction, setPendingAction] = useState<ActionBarAction | null>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const askReopenMut = useTaskAskReopen();
  const reopenMut = useTaskReopen();
  const reworkMut = useTaskRework();

  if (available.length > 0 && !available.includes(selectedAction)) {
    setSelectedAction(defaultAction);
  }

  const resetInput = useCallback(() => {
    clearDraft();
    if (textareaRef.current) textareaRef.current.style.height = 'auto';
  }, [clearDraft]);

  const handleSubmit = useCallback(async () => {
    const trimmed = text.trim();
    if (!trimmed || askReopenMut.isPending || reopenMut.isPending || reworkMut.isPending) return;
    const images = image ? [image] : undefined;
    setPendingAction(selectedAction);
    try {
      if (selectedAction === 'ask') {
        onAsk?.(trimmed, images);
        resetInput();
        setPendingAction(null);
        return;
      }
      if (selectedAction === 'reopen')
        await reopenMut.mutateAsync({ id: item.id, feedback: trimmed, images });
      else if (selectedAction === 'rework')
        await reworkMut.mutateAsync({ id: item.id, feedback: trimmed, images });
      resetInput();
    } catch {
      // toast handled by mutation hooks
    } finally {
      setPendingAction(null);
    }
  }, [text, image, selectedAction, item.id, onAsk, resetInput, askReopenMut, reopenMut, reworkMut]);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === 'Enter' && e.metaKey && text.trim()) {
        e.preventDefault();
        void handleSubmit();
      }
    },
    [text, handleSubmit],
  );

  const handlePaste = useCallback(
    (e: React.ClipboardEvent) => {
      const file = extractImageFromClipboard(e);
      if (file) setImageFile(file);
    },
    [setImageFile],
  );

  const hidden = isActionBarHidden(item);

  const config = ACTION_CONFIG[selectedAction];
  const isLoading =
    !!pendingAction || askReopenMut.isPending || reopenMut.isPending || reworkMut.isPending;
  const canSubmit = !hidden && text.trim().length > 0 && !isLoading;
  const submitState = isLoading ? 'pending' : canSubmit ? 'ready' : 'idle';

  const { data: askHistoryData } = useTaskAskHistory(item.id);
  const showAskReopen = shouldShowAskReopen(item, selectedAction, askHistoryData?.history);
  const askReopenState = askReopenMut.isPending ? 'pending' : showAskReopen ? 'ready' : 'hidden';

  return (
    <div
      className={`shrink-0 border-t-0 transition-[max-height,opacity] duration-150 ease-in-out ${hidden ? 'max-h-0 overflow-hidden opacity-0' : 'max-h-[320px] overflow-visible opacity-100'}`}
    >
      <div className="px-4 pt-3 pb-3">
        <div className="rounded-lg bg-muted px-3 py-2">
          {preview && image && (
            <ImageChip preview={preview} name={image.name} onRemove={removeImage} />
          )}

          <Textarea
            ref={textareaRef}
            className="min-h-[20px] max-h-[120px] w-full resize-none overflow-y-auto border-0 bg-transparent py-1 text-body leading-snug text-foreground shadow-none [scrollbar-width:none] focus-visible:ring-0 dark:bg-transparent"
            rows={1}
            placeholder={config.placeholder}
            value={text}
            onChange={(e) => {
              setText(e.target.value);
              e.target.style.height = 'auto';
              e.target.style.height = e.target.scrollHeight + 'px';
            }}
            onKeyDown={handleKeyDown}
            onPaste={handlePaste}
            disabled={isLoading}
          />

          <ActionBarFooter
            available={available}
            selectedAction={selectedAction}
            onActionChange={setSelectedAction}
            onImageSelect={setImageFile}
            isLoading={isLoading}
            submitState={submitState}
            askReopenState={askReopenState}
            onAskReopen={() => askReopenMut.mutate({ id: item.id })}
            onSubmit={() => void handleSubmit()}
          />
        </div>
      </div>
    </div>
  );
}
