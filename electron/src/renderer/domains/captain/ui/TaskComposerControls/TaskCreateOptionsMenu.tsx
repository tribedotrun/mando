import React, { useRef } from 'react';
import { ImagePlus, Plus } from 'lucide-react';
import type { TaskProvider } from '#renderer/global/types';
import { taskProviderLabel } from '#renderer/global/service/providerDisplay';
import { TaskCreateOptionSwitchRow } from '#renderer/domains/captain/ui/TaskComposerControls/TaskCreateOptionSwitchRow';
import { Button } from '#renderer/global/ui/primitives/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '#renderer/global/ui/primitives/dropdown-menu';

interface TaskCreateOptionsMenuProps {
  provider: TaskProvider;
  onProviderChange: (provider: TaskProvider) => void;
  planning: boolean;
  onPlanningChange: (planning: boolean) => void;
  globalAutoMerge: boolean;
  noAutoMerge: boolean;
  onNoAutoMergeChange: (noAutoMerge: boolean) => void;
  onImageSelect: (file: File) => void;
}

export function TaskCreateOptionsMenu({
  provider,
  onProviderChange,
  planning,
  onPlanningChange,
  globalAutoMerge,
  noAutoMerge,
  onNoAutoMergeChange,
  onImageSelect,
}: TaskCreateOptionsMenuProps): React.ReactElement {
  const fileRef = useRef<HTMLInputElement>(null);
  const optionsActive = planning || noAutoMerge || provider !== 'codex';
  const providers: TaskProvider[] = ['codex', 'claude'];

  return (
    <>
      <input
        ref={fileRef}
        type="file"
        accept="image/*"
        className="hidden"
        onChange={(event) => {
          const file = event.target.files?.[0];
          if (file) onImageSelect(file);
          event.target.value = '';
        }}
      />
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button
            variant={optionsActive ? 'outline' : 'ghost'}
            size="icon-sm"
            aria-label="Task creation options"
            title="Task creation options"
            data-testid="inline-task-options-menu"
            className={optionsActive ? 'text-foreground' : 'text-text-3'}
          >
            <Plus size={16} />
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent side="top" align="start" className="min-w-[210px]">
          <DropdownMenuItem
            onSelect={(event) => {
              event.preventDefault();
              fileRef.current?.click();
            }}
          >
            <ImagePlus size={16} />
            Attach image
          </DropdownMenuItem>

          <DropdownMenuSeparator />
          <DropdownMenuLabel className="text-caption uppercase tracking-[0.06em] text-text-3">
            Provider
          </DropdownMenuLabel>
          <DropdownMenuRadioGroup
            value={provider}
            onValueChange={(value) => onProviderChange(value as TaskProvider)}
          >
            {providers.map((item) => (
              <DropdownMenuRadioItem
                key={item}
                value={item}
                disabled={planning && item === 'codex'}
              >
                {taskProviderLabel(item)}
              </DropdownMenuRadioItem>
            ))}
          </DropdownMenuRadioGroup>
          {planning && (
            <div className="px-2 pb-1 text-caption text-text-3">Planning uses Claude Code.</div>
          )}

          <DropdownMenuSeparator />
          <TaskCreateOptionSwitchRow
            label="Plan mode"
            checked={planning}
            onCheckedChange={onPlanningChange}
          />
          {globalAutoMerge && (
            <TaskCreateOptionSwitchRow
              label="Auto-merge"
              checked={!noAutoMerge}
              onCheckedChange={(checked) => onNoAutoMergeChange(!checked)}
            />
          )}
        </DropdownMenuContent>
      </DropdownMenu>
    </>
  );
}
