import React from 'react';
import { Presentation } from 'lucide-react';
import { useNativeActions } from '#renderer/global/runtime/useNativeActions';
import { evidenceDeckPath } from '#renderer/domains/captain/service/evidenceHelpers';
import { Button } from '#renderer/global/ui/primitives/button';

/**
 * Opens the worktree's evidence deck in the default browser. The worker only
 * writes the deck once it has evidence to show, so a missing file surfaces as
 * the standard native-action toast rather than being hidden.
 */
export function EvidenceDeckButton({ worktree }: { worktree: string }): React.ReactElement {
  const { openLocalPath } = useNativeActions().files;

  return (
    <Button
      variant="ghost"
      size="xs"
      className="-ml-2 h-5 self-center justify-self-start text-caption text-text-2"
      onClick={() => openLocalPath(evidenceDeckPath(worktree))}
      data-testid="evidence-deck-open"
      title="Open the worker's evidence deck in your browser"
    >
      <Presentation className="text-text-3" />
      Open deck
    </Button>
  );
}
