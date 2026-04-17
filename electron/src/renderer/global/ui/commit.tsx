import * as React from 'react';

import { cn } from '#renderer/global/service/cn';
import { copyToClipboard } from '#renderer/global/service/utils';

interface CommitProps {
  hash: string;
  message: string;
  author?: string;
  timestamp?: string;
  className?: string;
}

export function Commit({
  hash,
  message,
  author,
  timestamp,
  className,
}: CommitProps): React.ReactElement {
  const shortHash = hash.slice(0, 7);

  const copyHash = () => {
    void copyToClipboard(hash, 'Copied SHA');
  };

  return (
    <span className={cn('inline-flex items-center gap-1.5 text-caption', className)}>
      <button
        type="button"
        onClick={copyHash}
        title="Copy full SHA"
        className="rounded bg-muted px-1 py-0.5 font-mono text-[11px] text-muted-foreground transition-colors hover:bg-muted/80 active:bg-muted/60"
      >
        {shortHash}
      </button>
      <span className="truncate text-foreground">{message}</span>
      {author && <span className="shrink-0 text-text-4">{author}</span>}
      {timestamp && <span className="shrink-0 text-text-4">{timestamp}</span>}
    </span>
  );
}
