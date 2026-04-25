import React from 'react';
import { ChevronDown } from 'lucide-react';
import {
  type ActionBarAction,
  ACTION_CONFIG,
} from '#renderer/domains/captain/service/actionBarHelpers';
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuTrigger,
} from '#renderer/global/ui/primitives/dropdown-menu';
import { Button } from '#renderer/global/ui/primitives/button';
import { TaskAttachmentButton } from '#renderer/domains/captain/ui/TaskAttachmentButton';
import { TaskSubmitButton } from '#renderer/domains/captain/ui/TaskComposerControls';
import { AskReopenButton, ImageChip } from '#renderer/domains/captain/ui/ActionBarToolbarParts';

export type ActionBarSubmitState = 'idle' | 'ready' | 'pending';
export type ActionBarAskReopenState = 'hidden' | 'ready' | 'pending';

interface ActionBarFooterProps {
  available: ActionBarAction[];
  selectedAction: ActionBarAction;
  onActionChange: (action: ActionBarAction) => void;
  onImageSelect: (file: File) => void;
  isLoading: boolean;
  submitState: ActionBarSubmitState;
  askReopenState: ActionBarAskReopenState;
  onAskReopen: () => void;
  onSubmit: () => void;
}

export function ActionBarFooter({
  available,
  selectedAction,
  onActionChange,
  onImageSelect,
  isLoading,
  submitState,
  askReopenState,
  onAskReopen,
  onSubmit,
}: ActionBarFooterProps): React.ReactElement {
  const hasMultipleActions = available.length > 1;
  const config = ACTION_CONFIG[selectedAction];
  const submitDisabled = submitState === 'idle' || submitState === 'pending';

  return (
    <div className="mt-1.5 flex items-center gap-2">
      {hasMultipleActions && (
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button
              variant="ghost"
              size="xs"
              disabled={isLoading}
              className="shrink-0 gap-1 text-muted-foreground"
            >
              {config.label}
              <ChevronDown size={10} className="opacity-60" />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent side="top" align="start" className="min-w-[120px]">
            {available.map((action) => (
              <DropdownMenuCheckboxItem
                key={action}
                checked={action === selectedAction}
                onSelect={() => onActionChange(action)}
              >
                {ACTION_CONFIG[action].label}
              </DropdownMenuCheckboxItem>
            ))}
          </DropdownMenuContent>
        </DropdownMenu>
      )}

      <TaskAttachmentButton
        onImageSelect={onImageSelect}
        size="icon-xs"
        disabled={isLoading}
        className="shrink-0 text-muted-foreground"
      />

      <AskReopenButton state={askReopenState} onAskReopen={onAskReopen} />

      <div className="flex-1" />

      <TaskSubmitButton
        disabled={submitDisabled}
        pending={submitState === 'pending'}
        onSubmit={onSubmit}
        variant={submitState === 'ready' || submitState === 'pending' ? 'default' : 'secondary'}
        ariaLabel="Submit action"
        tooltip="Submit ⌘↵"
        className="shrink-0 rounded-full transition-colors"
      />
    </div>
  );
}

export { ImageChip };
