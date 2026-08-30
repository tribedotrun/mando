import React, { useRef } from 'react';
import { ImagePlus, Plus } from 'lucide-react';
import { taskProviderLabel } from '#renderer/global/service/providerDisplay';
import { TaskCreateOptionSwitchRow } from '#renderer/domains/captain/ui/TaskComposerControls/TaskCreateOptionSwitchRow';
import type { PremiumTaskProvider } from '#renderer/domains/captain/runtime/useInlineTaskCreate.helpers';
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
  provider: PremiumTaskProvider;
  onProviderChange: (provider: PremiumTaskProvider) => void;
  useGlmWorker: boolean;
  defaultGlmWorker: boolean;
  onUseGlmWorkerChange: (useGlmWorker: boolean) => void;
  globalAutoMerge: boolean;
  noAutoMerge: boolean;
  onNoAutoMergeChange: (noAutoMerge: boolean) => void;
  onImageSelect: (file: File) => void;
}

export function TaskCreateOptionsMenu({
  provider,
  onProviderChange,
  useGlmWorker,
  defaultGlmWorker,
  onUseGlmWorkerChange,
  globalAutoMerge,
  noAutoMerge,
  onNoAutoMergeChange,
  onImageSelect,
}: TaskCreateOptionsMenuProps): React.ReactElement {
  const fileRef = useRef<HTMLInputElement>(null);
  // "Active" means any non-default choice — including turning GLM off when the
  // configured default is on, so the comparison is against `defaultGlmWorker`.
  const optionsActive = noAutoMerge || provider !== 'codex' || useGlmWorker !== defaultGlmWorker;
  const providers: PremiumTaskProvider[] = ['codex', 'claude'];

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
        <DropdownMenuContent side="top" align="start" className="min-w-[256px]">
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
          <DropdownMenuLabel className="text-label text-text-3">Agent</DropdownMenuLabel>
          <DropdownMenuRadioGroup
            value={provider}
            onValueChange={(value) => onProviderChange(value as PremiumTaskProvider)}
          >
            {providers.map((item) => (
              <DropdownMenuRadioItem
                key={item}
                value={item}
                onSelect={(event) => event.preventDefault()}
              >
                {taskProviderLabel(item)}
              </DropdownMenuRadioItem>
            ))}
          </DropdownMenuRadioGroup>
          <TaskCreateOptionSwitchRow
            label="GLM worker"
            description="Build the code with Z.ai GLM 5.2"
            checked={useGlmWorker}
            onCheckedChange={onUseGlmWorkerChange}
          />

          {globalAutoMerge && (
            <>
              <DropdownMenuSeparator />
              <TaskCreateOptionSwitchRow
                label="Auto-merge"
                description="Merge high-confidence PRs automatically"
                checked={!noAutoMerge}
                onCheckedChange={(checked) => onNoAutoMergeChange(!checked)}
              />
            </>
          )}
        </DropdownMenuContent>
      </DropdownMenu>
    </>
  );
}
