import React from 'react';
import type { EvidenceDeckView } from '#renderer/domains/captain/types/evidenceDeck';
import { Spinner } from '#renderer/domains/captain/ui/Spinner';

export function EvidenceDeckTab({
  deck,
  pending,
}: {
  deck: EvidenceDeckView | null;
  pending: boolean;
}): React.ReactElement {
  if (!deck) {
    return (
      <div className="flex h-full items-center justify-center gap-2 text-body text-text-3">
        {pending && <Spinner />}
        Loading deck…
      </div>
    );
  }

  return (
    <iframe
      key={deck.modifiedAtMs}
      title="Task evidence deck"
      srcDoc={deck.html}
      sandbox="allow-scripts"
      referrerPolicy="no-referrer"
      className="h-full min-h-0 w-full border-0 bg-background"
      data-testid="task-evidence-deck"
      allowFullScreen
    />
  );
}
