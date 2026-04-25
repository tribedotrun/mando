import React from 'react';
import { Kbd } from '#renderer/global/ui/primitives/kbd';
import { Tooltip, TooltipContent, TooltipTrigger } from '#renderer/global/ui/primitives/tooltip';

interface SidebarNavButtonProps {
  onClick: () => void;
  icon: React.ComponentType<{ size: number }>;
  label: string;
  shortcut: string;
}

export function SidebarNavButton({
  onClick,
  icon: Icon,
  label,
  shortcut,
}: SidebarNavButtonProps): React.ReactElement {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <button
          onClick={onClick}
          className="flex h-6 w-6 items-center justify-center rounded text-text-3 transition-colors hover:text-muted-foreground"
        >
          <Icon size={14} />
        </button>
      </TooltipTrigger>
      <TooltipContent
        side="bottom"
        className="flex items-center gap-3 px-3 py-2 text-sm font-medium"
      >
        {label} <Kbd>{shortcut}</Kbd>
      </TooltipContent>
    </Tooltip>
  );
}
