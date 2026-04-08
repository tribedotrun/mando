import React, { useState, useCallback, useRef } from 'react';
import { ArrowUp, ChevronDown } from 'lucide-react';
import { useQueryClient } from '@tanstack/react-query';
import log from '#renderer/logger';
import { reopenItem, reworkItem } from '#renderer/domains/captain/hooks/useApi';
import { useDraft } from '#renderer/global/hooks/useDraft';
import { useTaskStore } from '#renderer/domains/captain/stores/taskStore';
import { toast } from 'sonner';
import { FINALIZED_STATUSES, type TaskItem } from '#renderer/types';
import { canReopen, canRework, canAskAny, getErrorMessage } from '#renderer/utils';
import { invalidateTaskDetail } from '#renderer/queryClient';
import {
  DropdownMenu,
  DropdownMenuTrigger,
  DropdownMenuContent,
  DropdownMenuCheckboxItem,
} from '#renderer/components/ui/dropdown-menu';
import { Button } from '#renderer/components/ui/button';
import { Textarea } from '#renderer/components/ui/textarea';

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
  onAsk?: (question: string) => void;
}

export function TaskActionBar({ item, onAsk }: Props): React.ReactElement | null {
  const available = getAvailableActions(item);
  const defaultAction = getDefaultAction(item);
  const [selectedAction, setSelectedAction] = useState<Action>(defaultAction);
  const [text, setText, clearTextDraft] = useDraft(`mando:draft:action:${item.id}`);
  const [pendingAction, setPendingAction] = useState<Action | null>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const taskFetch = useTaskStore((s) => s.fetch);
  const queryClient = useQueryClient();
  // Sync selected action when task status changes -- only when there are valid actions.
  if (available.length > 0 && !available.includes(selectedAction)) {
    setSelectedAction(defaultAction);
  }

  const handleSubmit = useCallback(async () => {
    const trimmed = text.trim();
    if (!trimmed) return;
    setPendingAction(selectedAction);
    try {
      if (selectedAction === 'ask') {
        onAsk?.(trimmed);
        clearTextDraft();
        if (textareaRef.current) textareaRef.current.style.height = 'auto';
        setPendingAction(null);
        return;
      }
      if (selectedAction === 'reopen') await reopenItem(item.id, trimmed);
      else if (selectedAction === 'rework') await reworkItem(item.id, trimmed);
      void taskFetch();
      void invalidateTaskDetail(queryClient, item.id);
      const msg = selectedAction === 'reopen' ? 'Task reopened' : 'Rework requested';
      toast.success(msg);
      clearTextDraft();
      if (textareaRef.current) textareaRef.current.style.height = 'auto';
    } catch (err) {
      log.warn(`[TaskActionBar] ${selectedAction} failed for item ${item.id}:`, err);
      toast.error(getErrorMessage(err, `${selectedAction} failed`));
    } finally {
      setPendingAction(null);
    }
  }, [text, selectedAction, item.id, taskFetch, queryClient, onAsk, clearTextDraft]);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === 'Enter' && e.metaKey && text.trim()) {
        e.preventDefault();
        void handleSubmit();
      }
    },
    [text, handleSubmit],
  );

  // Determine visibility -- hidden states still reserve no space via height collapse.
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
  const isLoading = !!pendingAction;
  const canSubmit = !hidden && text.trim().length > 0 && !pendingAction;
  const showAccent = canSubmit || isLoading;

  return (
    <div
      className={`shrink-0 border-t-0 transition-[max-height,opacity] duration-150 ease-in-out ${hidden ? 'max-h-0 overflow-hidden opacity-0' : 'max-h-[200px] overflow-visible opacity-100'}`}
    >
      <div className="px-4 pt-3 pb-3">
        <div className="flex items-end gap-2 rounded-lg bg-muted px-3 py-2">
          <Textarea
            ref={textareaRef}
            className="min-h-[20px] max-h-[120px] flex-1 resize-none overflow-y-auto border-0 bg-transparent py-1 text-body leading-snug text-foreground shadow-none [scrollbar-width:none] focus-visible:ring-0 dark:bg-transparent"
            rows={1}
            placeholder={config.placeholder}
            value={text}
            onChange={(e) => {
              setText(e.target.value);
              e.target.style.height = 'auto';
              e.target.style.height = e.target.scrollHeight + 'px';
            }}
            onKeyDown={handleKeyDown}
            disabled={!!pendingAction}
          />

          {hasMultipleActions && (
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button
                  variant="ghost"
                  size="xs"
                  disabled={!!pendingAction}
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

          {/* Circular send button */}
          <Button
            onClick={() => void handleSubmit()}
            disabled={!canSubmit}
            variant={showAccent ? 'default' : 'secondary'}
            size="icon-xs"
            className="shrink-0 rounded-full transition-colors"
          >
            {isLoading ? (
              <svg className="animate-spin" width="14" height="14" viewBox="0 0 14 14" fill="none">
                <circle cx="7" cy="7" r="5.5" stroke="currentColor" strokeWidth="2" opacity="0.3" />
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
  // Ask is preferred default; fall back to first available action.
  if (available.includes('ask')) return 'ask';
  return available[0] ?? 'ask';
}
