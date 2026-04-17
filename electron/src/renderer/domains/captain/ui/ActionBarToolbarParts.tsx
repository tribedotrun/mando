import React from 'react';
import { RotateCcw, X } from 'lucide-react';
import { Button } from '#renderer/global/ui/button';
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from '#renderer/global/ui/tooltip';

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

export function ImageChip({
  preview,
  name,
  onRemove,
}: {
  preview: string;
  name: string;
  onRemove: () => void;
}): React.ReactElement {
  return (
    <div className="mb-1 flex items-center">
      <button
        onClick={onRemove}
        className="flex items-center gap-1.5 rounded-md bg-secondary/60 px-2 py-0.5 text-caption text-muted-foreground transition-colors hover:bg-secondary"
      >
        <img src={preview} alt="" className="h-4 w-4 rounded-sm object-cover" />
        <span className="max-w-[160px] truncate">{name}</span>
        <X size={10} className="shrink-0 opacity-60" />
      </button>
    </div>
  );
}
