import React from 'react';
import { useUpdateBanner } from '#renderer/global/runtime/useNativeActions';
import { Button } from '#renderer/global/ui/button';
import { Kbd } from '#renderer/global/ui/kbd';
import { Tooltip, TooltipContent, TooltipTrigger } from '#renderer/global/ui/tooltip';

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

export function SidebarUpdateButton(): React.ReactElement | null {
  const { updateReady, installing, installUpdate } = useUpdateBanner();
  if (!updateReady) return null;

  return (
    <Button
      size="xs"
      disabled={installing}
      onClick={installUpdate}
      className="absolute right-3 top-3 z-20"
      style={{ WebkitAppRegion: 'no-drag' } as React.CSSProperties}
    >
      {installing ? 'Installing…' : 'Update'}
    </Button>
  );
}
