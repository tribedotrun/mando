import React, { useState } from 'react';
import { cn } from '#renderer/global/service/cn';
import { ChevronDown, Copy, PanelLeft, ArrowLeft, ArrowRight, SquarePen } from 'lucide-react';
import { FinderIcon, CursorIcon } from '#renderer/global/ui/icons';
import { useMountEffect } from '#renderer/global/runtime/useMountEffect';
import { useNativeActions } from '#renderer/global/runtime/useNativeActions';
import { Tooltip, TooltipTrigger, TooltipContent } from '#renderer/global/ui/tooltip';
import { Kbd } from '#renderer/global/ui/kbd';
import { copyToClipboard } from '#renderer/global/service/utils';

export function CollapsedNavIcons({
  onToggleSidebar,
  onGoBack,
  onGoForward,
  onNewTask,
}: {
  onToggleSidebar?: () => void;
  onGoBack?: () => void;
  onGoForward?: () => void;
  onNewTask?: () => void;
}): React.ReactElement {
  return (
    <div
      className="flex shrink-0 items-center gap-1"
      style={{ WebkitAppRegion: 'no-drag' } as React.CSSProperties}
    >
      <Tooltip>
        <TooltipTrigger asChild>
          <button
            onClick={onToggleSidebar}
            aria-label="Toggle sidebar"
            className="flex h-6 w-6 items-center justify-center rounded text-text-3 transition-colors hover:text-muted-foreground"
          >
            <PanelLeft size={14} />
          </button>
        </TooltipTrigger>
        <TooltipContent
          side="bottom"
          className="flex items-center gap-3 px-3 py-2 text-sm font-medium"
        >
          Toggle sidebar <Kbd>&#8984;B</Kbd>
        </TooltipContent>
      </Tooltip>
      <Tooltip>
        <TooltipTrigger asChild>
          <button
            onClick={onGoBack}
            aria-label="Back"
            className="flex h-6 w-6 items-center justify-center rounded text-text-3 transition-colors hover:text-muted-foreground"
          >
            <ArrowLeft size={14} />
          </button>
        </TooltipTrigger>
        <TooltipContent
          side="bottom"
          className="flex items-center gap-3 px-3 py-2 text-sm font-medium"
        >
          Back <Kbd>&#8984;[</Kbd>
        </TooltipContent>
      </Tooltip>
      <Tooltip>
        <TooltipTrigger asChild>
          <button
            onClick={onGoForward}
            aria-label="Forward"
            className="flex h-6 w-6 items-center justify-center rounded text-text-3 transition-colors hover:text-muted-foreground"
          >
            <ArrowRight size={14} />
          </button>
        </TooltipTrigger>
        <TooltipContent
          side="bottom"
          className="flex items-center gap-3 px-3 py-2 text-sm font-medium"
        >
          Forward <Kbd>&#8984;]</Kbd>
        </TooltipContent>
      </Tooltip>
      <Tooltip>
        <TooltipTrigger asChild>
          <button
            onClick={onNewTask}
            aria-label="New task"
            className="flex h-6 w-6 items-center justify-center rounded text-text-3 transition-colors hover:text-muted-foreground"
          >
            <SquarePen size={14} />
          </button>
        </TooltipTrigger>
        <TooltipContent side="bottom" className="px-3 py-2 text-sm font-medium">
          New task
        </TooltipContent>
      </Tooltip>
    </div>
  );
}

export function OpenMenu({ worktreePath }: { worktreePath: string | null }): React.ReactElement {
  const [open, setOpen] = useState(false);
  const { openInFinder, openInCursor } = useNativeActions();
  const disabled = !worktreePath;

  useMountEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        setOpen((prev) => {
          if (prev) e.stopPropagation();
          return false;
        });
      }
    };
    document.addEventListener('keydown', onKey);
    return () => document.removeEventListener('keydown', onKey);
  });

  return (
    <div className="relative" style={{ WebkitAppRegion: 'no-drag' } as React.CSSProperties}>
      {/* Split button: default action (Cursor) + dropdown chevron */}
      <div
        className={cn(
          'flex items-center rounded-md border border-border',
          disabled && 'pointer-events-none opacity-40',
        )}
      >
        <button
          onClick={() => worktreePath && openInCursor(worktreePath)}
          disabled={disabled}
          className="flex items-center rounded-l-md px-2 py-1 transition-colors hover:bg-accent"
          aria-label="Open in Cursor"
        >
          <CursorIcon size={14} />
        </button>
        <div className="h-4 w-px bg-border" />
        <button
          onClick={() => !disabled && setOpen((v) => !v)}
          disabled={disabled}
          className="flex items-center rounded-r-md px-1.5 py-1 text-muted-foreground transition-colors hover:bg-accent hover:text-foreground"
          aria-label="More open options"
          aria-haspopup="true"
          aria-expanded={open}
        >
          <ChevronDown size={12} />
        </button>
      </div>
      {open && (
        <>
          <div className="fixed inset-0 z-40" onClick={() => setOpen(false)} />
          <div className="absolute right-0 top-full z-50 mt-1 min-w-[200px] rounded-md border border-border bg-popover py-1 shadow-lg">
            <button
              className="flex w-full items-center gap-2.5 px-3 py-2 text-[13px] text-popover-foreground transition-colors hover:bg-accent"
              onClick={() => {
                openInFinder(worktreePath!);
                setOpen(false);
              }}
            >
              <FinderIcon size={16} />
              <span className="flex-1 text-left">Finder</span>
            </button>
            <button
              className="flex w-full items-center gap-2.5 px-3 py-2 text-[13px] text-popover-foreground transition-colors hover:bg-accent"
              onClick={() => {
                openInCursor(worktreePath!);
                setOpen(false);
              }}
            >
              <CursorIcon size={16} />
              <span className="flex-1 text-left">Cursor</span>
            </button>
            <div className="my-1 h-px bg-border" />
            <button
              className="flex w-full items-center gap-2.5 px-3 py-2 text-[13px] text-popover-foreground transition-colors hover:bg-accent"
              onClick={() => {
                void copyToClipboard(worktreePath!, 'Path copied');
                setOpen(false);
              }}
            >
              <Copy size={15} className="shrink-0 text-muted-foreground" />
              <span className="flex-1 text-left">Copy path</span>
              <span className="flex items-center gap-0.5 text-text-3">
                <Kbd>&#8984;</Kbd>
                <Kbd>&#8679;</Kbd>
                <Kbd>C</Kbd>
              </span>
            </button>
          </div>
        </>
      )}
    </div>
  );
}
