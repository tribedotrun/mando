import React from 'react';
import { PanelLeft, ArrowLeft, ArrowRight, SquarePen } from 'lucide-react';
import { Tooltip, TooltipTrigger, TooltipContent } from '#renderer/global/ui/tooltip';
import { Kbd } from '#renderer/global/ui/kbd';

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
    <div className="flex shrink-0 items-center gap-1" style={{ WebkitAppRegion: 'no-drag' }}>
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
