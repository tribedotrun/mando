import React, { useState } from 'react';
import { cn } from '#renderer/global/service/cn';
import { ChevronDown, Copy } from 'lucide-react';
import { FinderIcon, CursorIcon } from '#renderer/global/ui/primitives/icons';
import { useMountEffect } from '#renderer/global/runtime/useMountEffect';
import { useNativeActions } from '#renderer/global/runtime/useNativeActions';
import { Kbd } from '#renderer/global/ui/primitives/kbd';
import { copyToClipboard } from '#renderer/global/runtime/useFeedback';

export function AppHeaderOpenMenu({
  worktreePath,
}: {
  worktreePath: string | null;
}): React.ReactElement {
  const [open, setOpen] = useState(false);
  const { openInFinder, openInCursor } = useNativeActions().files;
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
    <div className="relative" style={{ WebkitAppRegion: 'no-drag' }}>
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
