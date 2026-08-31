import React, { useId, useRef } from 'react';
import { Switch } from '#renderer/global/ui/primitives/switch';
import { ImagePlus, Plus, ArrowUp } from 'lucide-react';
import { taskProviderLabel } from '#renderer/global/service/providerDisplay';
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
import { shortRepo } from '#renderer/global/service/utils';
import { Combobox } from '#renderer/global/ui/primitives/combobox';
import { SpinnerIcon } from '#renderer/domains/captain/ui/SpinnerIcon';
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from '#renderer/global/ui/primitives/tooltip';

interface TaskCreateOptionSwitchRowProps {
  label: string;
  description?: string;
  checked: boolean;
  onCheckedChange: (checked: boolean) => void;
}

function TaskCreateOptionSwitchRow({
  label,
  description,
  checked,
  onCheckedChange,
}: TaskCreateOptionSwitchRowProps): React.ReactElement {
  const id = useId();

  return (
    <div className="flex items-center justify-between gap-3 rounded-sm px-2 py-1.5 outline-hidden transition-colors hover:bg-accent hover:text-accent-foreground focus-within:bg-accent focus-within:text-accent-foreground">
      <label htmlFor={id} className="min-w-0 flex-1 cursor-pointer select-none">
        <span className="block text-sm">{label}</span>
        {description && (
          <span className="mt-0.5 block text-caption text-muted-foreground">{description}</span>
        )}
      </label>
      <Switch
        id={id}
        size="sm"
        checked={checked}
        onCheckedChange={onCheckedChange}
        aria-label={label}
      />
    </div>
  );
}

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

interface TaskProjectSelectProps {
  projects: string[];
  value: string;
  onValueChange: (value: string) => void;
  testId: string;
}

export function TaskProjectSelect({
  projects,
  value,
  onValueChange,
  testId,
}: TaskProjectSelectProps): React.ReactElement | null {
  if (projects.length === 0) return null;

  return (
    <Combobox
      data-testid={testId}
      value={value}
      onValueChange={onValueChange}
      options={projects.map((item) => ({
        value: item,
        label: shortRepo(item),
      }))}
      placeholder="Project..."
      searchPlaceholder="Search projects..."
      emptyText="No projects found."
    />
  );
}

interface TaskSubmitButtonProps {
  disabled: boolean;
  pending: boolean;
  onSubmit: () => void;
  testId?: string;
  tooltip?: string;
  className?: string;
  ariaLabel?: string;
  variant?: 'default' | 'secondary';
}

export function TaskSubmitButton({
  disabled,
  pending,
  onSubmit,
  testId,
  tooltip = 'Create ⌘↵',
  className,
  ariaLabel = 'Create task',
  variant = 'default',
}: TaskSubmitButtonProps): React.ReactElement {
  return (
    <TooltipProvider delayDuration={300}>
      <Tooltip>
        <TooltipTrigger asChild>
          <Button
            data-testid={testId}
            onClick={onSubmit}
            disabled={disabled}
            variant={variant}
            size="icon-xs"
            aria-label={ariaLabel}
            className={className ?? 'shrink-0 rounded-full transition-colors'}
          >
            {pending ? <SpinnerIcon /> : <ArrowUp size={14} strokeWidth={2} />}
          </Button>
        </TooltipTrigger>
        <TooltipContent side="top" className="text-xs">
          {tooltip}
        </TooltipContent>
      </Tooltip>
    </TooltipProvider>
  );
}
