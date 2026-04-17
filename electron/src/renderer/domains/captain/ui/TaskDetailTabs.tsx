import React, { useState } from 'react';
import { Check, Copy, X } from 'lucide-react';
import type { TaskItem } from '#renderer/global/types';
import { copyToClipboard, shortenPath } from '#renderer/global/service/utils';
import { useCopyFeedback } from '#renderer/global/runtime/useCopyFeedback';
import { PrSections } from '#renderer/domains/captain/ui/PrSections';
import { PrMarkdown } from '#renderer/global/ui/PrMarkdown';
import {
  Dialog,
  DialogContentPlain,
  DialogHeader,
  DialogTitle,
  DialogClose,
} from '#renderer/global/ui/dialog';
import {
  Tooltip,
  TooltipTrigger,
  TooltipContent,
  TooltipProvider,
} from '#renderer/global/ui/tooltip';
import { Button } from '#renderer/global/ui/button';
import { Skeleton } from '#renderer/global/ui/skeleton';

/* -- PR tab -- */

export function PrTab({
  item,
  prBody,
  prPending,
}: {
  item: TaskItem;
  prBody: { summary: string | null } | undefined;
  prPending: boolean;
}): React.ReactElement {
  if (!item.pr_number) {
    return <div className="text-caption text-text-3">No PR associated with this task</div>;
  }
  if (prPending && !prBody) {
    return (
      <div className="min-h-[120px] space-y-3">
        <Skeleton className="h-4 w-3/4" />
        <Skeleton className="h-4 w-1/2" />
        <Skeleton className="h-4 w-2/3" />
      </div>
    );
  }
  if (!prBody?.summary) {
    return <div className="text-caption italic text-text-3">No PR description available</div>;
  }
  return <PrSections text={prBody.summary} />;
}

/* -- Info tab -- */

export function InfoTab({ item }: { item: TaskItem }): React.ReactElement {
  return (
    <div className="space-y-5">
      <div className="grid grid-cols-[auto_1fr] items-baseline gap-x-6 gap-y-2.5">
        <span className="text-caption text-text-4">ID</span>
        <span className="font-mono text-caption text-text-2">#{item.id}</span>

        {item.worktree && (
          <>
            <span className="text-caption text-text-4">Worktree</span>
            <CopyValue value={item.worktree} display={shortenPath(item.worktree)} />
          </>
        )}

        {item.branch && (
          <>
            <span className="text-caption text-text-4">Branch</span>
            <CopyValue value={item.branch} />
          </>
        )}

        {item.plan && (
          <>
            <span className="text-caption text-text-4">Plan</span>
            <CopyValue value={item.plan} display={shortenPath(item.plan)} />
          </>
        )}

        {item.no_auto_merge && (
          <>
            <span className="text-caption text-text-4">Auto-merge</span>
            <span className="text-caption text-text-2">Disabled</span>
          </>
        )}
      </div>

      {item.original_prompt && (
        <div>
          <div className="mb-1.5 text-caption text-text-4">Original Request</div>
          <p className="text-body leading-relaxed text-text-2 [overflow-wrap:anywhere]">
            {item.original_prompt}
          </p>
        </div>
      )}
    </div>
  );
}

function CopyValue({ value, display }: { value: string; display?: string }): React.ReactElement {
  const { copied, markCopied } = useCopyFeedback();
  const [copying, setCopying] = useState(false);
  return (
    <span className="inline-flex items-center gap-2 text-code text-muted-foreground">
      <span className="min-w-0 break-all">{display ?? value}</span>
      <TooltipProvider>
        <Tooltip>
          <TooltipTrigger asChild>
            <Button
              variant="ghost"
              size="icon-xs"
              disabled={copying}
              onClick={() => {
                setCopying(true);
                void copyToClipboard(value)
                  .then((ok) => {
                    if (ok) markCopied();
                  })
                  .finally(() => setCopying(false));
              }}
              className="h-5 w-5"
            >
              {copied ? (
                <Check size={12} color="var(--success)" />
              ) : (
                <Copy size={12} color="var(--text-4)" />
              )}
            </Button>
          </TooltipTrigger>
          <TooltipContent>{copied ? 'Copied!' : 'Copy'}</TooltipContent>
        </Tooltip>
      </TooltipProvider>
    </span>
  );
}

/* -- Context modal -- */

export function ContextModal({
  context,
  onClose,
}: {
  context: string;
  onClose: () => void;
}): React.ReactElement {
  return (
    <Dialog
      open={true}
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
    >
      <DialogContentPlain
        data-testid="context-modal"
        className="flex max-h-[70vh] w-[560px] max-w-[90vw] flex-col p-0"
      >
        <div className="flex shrink-0 items-center justify-between px-5 pt-4 pb-3">
          <DialogHeader className="flex-1">
            <DialogTitle className="mb-0">Context</DialogTitle>
          </DialogHeader>
          <DialogClose asChild>
            <Button variant="ghost" size="icon-xs">
              <X size={14} />
            </Button>
          </DialogClose>
        </div>
        <div className="min-w-0 overflow-y-auto px-5 pb-5 [overflow-wrap:anywhere]">
          <PrMarkdown text={context} />
        </div>
      </DialogContentPlain>
    </Dialog>
  );
}
