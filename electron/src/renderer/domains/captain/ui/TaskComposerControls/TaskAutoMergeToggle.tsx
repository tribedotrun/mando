import React from 'react';
import { Switch } from '#renderer/global/ui/primitives/switch';

interface TaskAutoMergeToggleProps {
  checked: boolean;
  onCheckedChange: (value: boolean) => void;
  className?: string;
}

export function TaskAutoMergeToggle({
  checked,
  onCheckedChange,
  className,
}: TaskAutoMergeToggleProps): React.ReactElement {
  return (
    <label className={className ?? 'flex items-center gap-1.5 text-[12px] text-muted-foreground'}>
      <Switch checked={checked} onCheckedChange={onCheckedChange} className="scale-75" />
      Skip auto-merge
    </label>
  );
}
