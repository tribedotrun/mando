import React, { useState } from 'react';
import { Check, Copy } from 'lucide-react';
import { copyToClipboard } from '#renderer/global/runtime/useFeedback';
import { useCopyFeedback } from '#renderer/global/runtime/useCopyFeedback';
import {
  Tooltip,
  TooltipTrigger,
  TooltipContent,
  TooltipProvider,
} from '#renderer/global/ui/primitives/tooltip';
import { Button } from '#renderer/global/ui/primitives/button';

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
                void (async () => {
                  setCopying(true);
                  try {
                    const ok = await copyToClipboard(value);
                    if (ok) markCopied();
                  } finally {
                    setCopying(false);
                  }
                })();
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
