import React, { useState } from 'react';
import { Check, Copy, X } from 'lucide-react';
import { copyToClipboard } from '#renderer/global/runtime/useFeedback';
import { useCopyFeedback } from '#renderer/global/runtime/useCopyFeedback';
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

export function CopyValue({
  value,
  display,
}: {
  value: string;
  display?: string;
}): React.ReactElement {
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
              aria-label={copied ? 'Copied' : 'Copy value'}
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
            <Button variant="ghost" size="icon-xs" aria-label="Close context">
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
