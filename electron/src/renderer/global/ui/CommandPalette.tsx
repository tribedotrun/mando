import React from 'react';
import { CommandPaletteInner } from '#renderer/global/ui/CommandPaletteInner';

interface Props {
  open: boolean;
  onClose: () => void;
  onAction: (action: string) => void;
}

export function CommandPalette({ open, onClose, onAction }: Props): React.ReactElement | null {
  if (!open) return null;
  return <CommandPaletteInner onClose={onClose} onAction={onAction} />;
}
