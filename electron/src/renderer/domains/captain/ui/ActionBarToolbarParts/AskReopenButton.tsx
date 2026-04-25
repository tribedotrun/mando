import React from 'react';
import { RotateCcw } from 'lucide-react';
import { Button } from '#renderer/global/ui/primitives/button';
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from '#renderer/global/ui/primitives/tooltip';

export function AskReopenButton({
  state,
  onAskReopen,
}: {
  state: 'hidden' | 'ready' | 'pending';
  onAskReopen: () => void;
}): React.ReactElement | null {
  if (state === 'hidden') return null;

  if (state === 'pending') {
    return (
      <Button variant="outline" size="xs" disabled className="shrink-0 text-muted-foreground">
        <RotateCcw size={12} className="animate-spin" />
        Reopening...
      </Button>
    );
  }

  return (
    <TooltipProvider delayDuration={300}>
      <Tooltip>
        <TooltipTrigger asChild>
          <Button
            variant="outline"
            size="icon-xs"
            onClick={onAskReopen}
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
  );
}
