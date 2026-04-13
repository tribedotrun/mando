import React, { useState, useCallback, useRef } from 'react';
import { ArrowUp, ChevronDown, Paperclip, RotateCcw, X } from 'lucide-react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { fetchAskHistory } from '#renderer/domains/captain/hooks/useApi';
import { useTaskAskReopen, useTaskReopen, useTaskRework } from '#renderer/hooks/mutations';
import { useDraft } from '#renderer/global/hooks/useDraft';
import { useMountEffect } from '#renderer/global/hooks/useMountEffect';
import { FINALIZED_STATUSES, type TaskItem } from '#renderer/types';
import { canReopen, canRework, canAskAny } from '#renderer/utils';
import { invalidateTaskDetail } from '#renderer/queryClient';
import { queryKeys } from '#renderer/queryKeys';
import {
  DropdownMenu,
  DropdownMenuTrigger,
  DropdownMenuContent,
  DropdownMenuCheckboxItem,
} from '#renderer/components/ui/dropdown-menu';
import { Button } from '#renderer/components/ui/button';
import { Textarea } from '#renderer/components/ui/textarea';
import {
  Tooltip,
  TooltipTrigger,
  TooltipContent,
  TooltipProvider,
} from '#renderer/components/ui/tooltip';

type Action = 'ask' | 'reopen' | 'rework';

const ACTION_CONFIG: Record<
  Action,
  { label: string; placeholder: string; requiresInput: boolean }
> = {
  ask: { label: 'Ask', placeholder: 'Ask about this task...', requiresInput: true },
  reopen: { label: 'Reopen', placeholder: 'Feedback for reopen...', requiresInput: true },
  rework: { label: 'Rework', placeholder: 'Feedback for rework...', requiresInput: true },
};

interface Props {
  item: TaskItem;
  onAsk?: (question: string, images?: File[]) => void;
}

export function TaskActionBar({ item, onAsk }: Props): React.ReactElement | null {
  const available = getAvailableActions(item);
  const defaultAction = getDefaultAction(item);
  const [selectedAction, setSelectedAction] = useState<Action>(defaultAction);
  const [text, setText, clearTextDraft] = useDraft(`mando:draft:action:${item.id}`);
  const [pendingAction, setPendingAction] = useState<Action | null>(null);
  const [image, setImage] = useState<File | null>(null);
  const [preview, setPreview] = useState<string | null>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const fileRef = useRef<HTMLInputElement>(null);
  const queryClient = useQueryClient();
  const askReopenMut = useTaskAskReopen();
  const reopenMut = useTaskReopen();
  const reworkMut = useTaskRework();

  // Sync selected action when task status changes.
  if (available.length > 0 && !available.includes(selectedAction)) {
    setSelectedAction(defaultAction);
  }

  // Clean up preview URL on unmount.
  const previewRef = useRef(preview);
  previewRef.current = preview;
  useMountEffect(() => {
    return () => {
      if (previewRef.current) URL.revokeObjectURL(previewRef.current);
    };
  });

  // Reset image when the active task changes (render-time sync).
  const prevItemId = useRef(item.id);
  if (prevItemId.current !== item.id) {
    prevItemId.current = item.id;
    if (previewRef.current) URL.revokeObjectURL(previewRef.current);
    setImage(null);
    setPreview(null);
    previewRef.current = null;
  }

  const setImageFile = useCallback((file: File) => {
    if (previewRef.current) URL.revokeObjectURL(previewRef.current);
    setImage(file);
    const url = URL.createObjectURL(file);
    setPreview(url);
    previewRef.current = url;
  }, []);

  const removeImage = useCallback(() => {
    if (previewRef.current) URL.revokeObjectURL(previewRef.current);
    setImage(null);
    setPreview(null);
    previewRef.current = null;
  }, []);

  const resetInput = useCallback(() => {
    clearTextDraft();
    removeImage();
    if (textareaRef.current) textareaRef.current.style.height = 'auto';
  }, [clearTextDraft, removeImage]);

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
      void invalidateTaskDetail(queryClient, item.id);
      resetInput();
    } catch {
      // toast handled by mutation hooks
    } finally {
      setPendingAction(null);
    }
  }, [
    text,
    image,
    selectedAction,
    item.id,
    queryClient,
    onAsk,
    resetInput,
    askReopenMut,
    reopenMut,
    reworkMut,
  ]);

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
      for (const clipItem of e.clipboardData.items) {
        if (!clipItem.type.startsWith('image/')) continue;
        e.preventDefault();
        const file = clipItem.getAsFile();
        if (file) setImageFile(file);
        return;
      }
    },
    [setImageFile],
  );

  // Determine visibility.
  const isFinalized = FINALIZED_STATUSES.includes(item.status);
  const hidden =
    isFinalized ||
    item.status === 'needs-clarification' ||
    item.status === 'captain-reviewing' ||
    item.status === 'new' ||
    item.status === 'queued' ||
    available.length === 0;

  const config = ACTION_CONFIG[selectedAction];
  const hasMultipleActions = available.length > 1;
  const isLoading =
    !!pendingAction || askReopenMut.isPending || reopenMut.isPending || reworkMut.isPending;
  const canSubmit = !hidden && text.trim().length > 0 && !isLoading;
  const showAccent = canSubmit || isLoading;

  // "Reopen from Q&A" -- subscribe to ask history reactively.
  const { data: askHistoryData } = useQuery({
    queryKey: queryKeys.tasks.askHistory(item.id),
    queryFn: () => fetchAskHistory(item.id),
  });
  const hasSuccessfulQA = askHistoryData?.history?.some(
    (m) => m.role === 'assistant' && !m.content.startsWith('Error: '),
  );
  const showAskReopen =
    selectedAction === 'ask' &&
    (item.status === 'awaiting-review' || item.status === 'escalated') &&
    !!hasSuccessfulQA &&
    !!item.session_ids?.ask;

  return (
    <div
      className={`shrink-0 border-t-0 transition-[max-height,opacity] duration-150 ease-in-out ${hidden ? 'max-h-0 overflow-hidden opacity-0' : 'max-h-[320px] overflow-visible opacity-100'}`}
    >
      <div className="px-4 pt-3 pb-3">
        <div className="rounded-lg bg-muted px-3 py-2">
          {/* Image chip */}
          {preview && image && (
            <div className="mb-1 flex items-center">
              <button
                onClick={removeImage}
                className="flex items-center gap-1.5 rounded-md bg-secondary/60 px-2 py-0.5 text-caption text-muted-foreground transition-colors hover:bg-secondary"
              >
                <img src={preview} alt="" className="h-4 w-4 rounded-sm object-cover" />
                <span className="max-w-[160px] truncate">{image.name}</span>
                <X size={10} className="shrink-0 opacity-60" />
              </button>
            </div>
          )}

          {/* Text input */}
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

          {/* Toolbar row: action selector, paperclip, reopen button, send */}
          <div className="mt-1.5 flex items-center gap-2">
            {hasMultipleActions && (
              <DropdownMenu>
                <DropdownMenuTrigger asChild>
                  <Button
                    variant="ghost"
                    size="xs"
                    disabled={isLoading}
                    className="shrink-0 gap-1 text-muted-foreground"
                  >
                    {config.label}
                    <ChevronDown size={10} className="opacity-60" />
                  </Button>
                </DropdownMenuTrigger>
                <DropdownMenuContent side="top" align="start" className="min-w-[120px]">
                  {available.map((action) => (
                    <DropdownMenuCheckboxItem
                      key={action}
                      checked={action === selectedAction}
                      onSelect={() => setSelectedAction(action)}
                    >
                      {ACTION_CONFIG[action].label}
                    </DropdownMenuCheckboxItem>
                  ))}
                </DropdownMenuContent>
              </DropdownMenu>
            )}

            {/* Attach image */}
            <input
              ref={fileRef}
              type="file"
              accept="image/*"
              className="hidden"
              onChange={(e) => {
                const file = e.target.files?.[0];
                if (file) setImageFile(file);
                e.target.value = '';
              }}
            />
            <Button
              variant="ghost"
              size="icon-xs"
              onClick={() => fileRef.current?.click()}
              disabled={isLoading}
              aria-label="Attach image"
              className="shrink-0 text-muted-foreground"
            >
              <Paperclip size={14} />
            </Button>

            {showAskReopen &&
              (askReopenMut.isPending ? (
                <Button
                  variant="outline"
                  size="xs"
                  disabled
                  className="shrink-0 text-muted-foreground"
                >
                  <RotateCcw size={12} className="animate-spin" />
                  Reopening...
                </Button>
              ) : (
                <TooltipProvider delayDuration={300}>
                  <Tooltip>
                    <TooltipTrigger asChild>
                      <Button
                        variant="outline"
                        size="icon-xs"
                        onClick={() => askReopenMut.mutate({ id: item.id })}
                        className="shrink-0 text-muted-foreground"
                      >
                        <RotateCcw size={12} />
                      </Button>
                    </TooltipTrigger>
                    <TooltipContent side="top" className="text-xs">
                      Reopen from Q&A
                    </TooltipContent>
                  </Tooltip>
                </TooltipProvider>
              ))}

            <div className="flex-1" />

            {/* Send button */}
            <Button
              onClick={() => void handleSubmit()}
              disabled={!canSubmit}
              variant={showAccent ? 'default' : 'secondary'}
              size="icon-xs"
              className="shrink-0 rounded-full transition-colors"
            >
              {isLoading ? (
                <svg
                  className="animate-spin"
                  width="14"
                  height="14"
                  viewBox="0 0 14 14"
                  fill="none"
                >
                  <circle
                    cx="7"
                    cy="7"
                    r="5.5"
                    stroke="currentColor"
                    strokeWidth="2"
                    opacity="0.3"
                  />
                  <path
                    d="M12.5 7a5.5 5.5 0 0 0-5.5-5.5"
                    stroke="currentColor"
                    strokeWidth="2"
                    strokeLinecap="round"
                  />
                </svg>
              ) : (
                <ArrowUp size={14} strokeWidth={2} />
              )}
            </Button>
          </div>
        </div>
      </div>
    </div>
  );
}

function getAvailableActions(item: TaskItem): Action[] {
  const actions: Action[] = [];
  if (canAskAny(item)) actions.push('ask');
  if (canReopen(item)) actions.push('reopen');
  if (canRework(item)) actions.push('rework');
  return actions;
}

function getDefaultAction(item: TaskItem): Action {
  const available = getAvailableActions(item);
  if (available.includes('ask')) return 'ask';
  return available[0] ?? 'ask';
}
