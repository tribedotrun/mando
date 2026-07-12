import React from 'react';
import { CirclePause, CirclePlay, KeyRound, Trash2 } from 'lucide-react';
import { Button } from '#renderer/global/ui/primitives/button';

interface CredentialActionsProps {
  isDisabled: boolean;
  onRemove: () => void;
  onSetDisabled: (disabled: boolean) => void;
  onUpdateAuth: () => void;
  removePending: boolean;
  setDisabledPending: boolean;
}

export function CredentialActions({
  isDisabled,
  onRemove,
  onSetDisabled,
  onUpdateAuth,
  removePending,
  setDisabledPending,
}: CredentialActionsProps): React.ReactElement {
  return (
    <div className="ml-2 flex shrink-0 items-center gap-1">
      <Button
        variant="ghost"
        size="icon"
        className="text-muted-foreground hover:text-foreground"
        onClick={onUpdateAuth}
        title="Update auth"
        aria-label="Update auth"
      >
        <KeyRound size={14} />
      </Button>
      <Button
        variant="ghost"
        size="icon"
        className="text-muted-foreground hover:text-foreground"
        disabled={setDisabledPending}
        onClick={() => onSetDisabled(!isDisabled)}
        title={isDisabled ? 'Enable credential' : 'Disable credential'}
        aria-label={isDisabled ? 'Enable credential' : 'Disable credential'}
      >
        {isDisabled ? <CirclePlay size={14} /> : <CirclePause size={14} />}
      </Button>
      <Button
        variant="ghost"
        size="icon"
        className="text-muted-foreground hover:text-destructive"
        disabled={removePending}
        onClick={onRemove}
        title="Remove credential"
        aria-label="Remove credential"
      >
        <Trash2 size={14} />
      </Button>
    </div>
  );
}
