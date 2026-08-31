import React from 'react';
import { ArrowUp, Loader2 } from 'lucide-react';
import type { TaskItem } from '#renderer/global/types';
import { cn } from '#renderer/global/service/cn';
import {
  useReopenReworkComposer,
  type ReopenReworkIntent,
} from '#renderer/domains/captain/runtime/useReopenReworkComposer';

export function ReopenReworkComposer({
  item,
  onSend,
  isPending,
}: {
  item: TaskItem;
  onSend: (message: string, intent: ReopenReworkIntent) => void;
  isPending: boolean;
}): React.ReactElement | null {
  const composer = useReopenReworkComposer({ item, onSend, isPending });
  const { text, events, intent } = composer;
  if (!intent.canReopen && !intent.canRework) return null;

  const showSelect = intent.canReopen && intent.canRework;
  const label = intent.value === 'reopen' ? 'Reopen' : 'Rework';

  return (
    <div className="bg-background px-2 pb-1.5">
      <div className="rounded-xl border border-accent/40 bg-surface-1 transition-colors focus-within:border-text-3">
        <textarea
          ref={text.textareaRef}
          value={text.input}
          onChange={text.handleInput}
          onKeyDown={events.handleKeyDown}
          placeholder={
            intent.value === 'reopen'
              ? 'Describe what to fix (resumes the worker)...'
              : 'Describe what to redo (fresh worker + new branch)...'
          }
          rows={2}
          className="min-h-[52px] max-h-[256px] w-full resize-none border-0 bg-transparent px-3.5 pt-3 pb-0 text-body leading-5 text-text-1 placeholder:text-text-3 focus:outline-none"
        />
        <div className="flex items-center justify-between px-1.5 pb-1.5">
          <div>
            {showSelect ? (
              <select
                value={intent.value}
                onChange={(event) => intent.set(event.target.value as ReopenReworkIntent)}
                className="cursor-pointer appearance-none rounded-md bg-transparent py-1 pr-4 pl-2 text-body text-text-3 hover:text-text-1 focus:outline-none"
                style={{
                  backgroundImage: `url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='10' height='10' viewBox='0 0 24 24' fill='none' stroke='%23666' stroke-width='2.5' stroke-linecap='round' stroke-linejoin='round'%3E%3Cpath d='m6 9 6 6 6-6'/%3E%3C/svg%3E")`,
                  backgroundRepeat: 'no-repeat',
                  backgroundPosition: 'right 3px center',
                }}
              >
                <option value="reopen">Reopen</option>
                <option value="rework">Rework</option>
              </select>
            ) : (
              <span className="py-1 pl-2 text-body text-text-4">{label}</span>
            )}
          </div>
          <button
            type="button"
            onClick={events.handleSubmit}
            disabled={!composer.canSubmit}
            aria-label={label}
            className={cn(
              'flex h-7 w-7 items-center justify-center rounded-lg transition-all duration-150',
              composer.canSubmit ? 'bg-text-1 text-background hover:opacity-80' : 'text-text-4',
            )}
          >
            {isPending ? <Loader2 size={14} className="animate-spin" /> : <ArrowUp size={14} />}
          </button>
        </div>
      </div>
    </div>
  );
}
