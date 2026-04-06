import React, { useState } from 'react';
import { copyToClipboard } from '#renderer/utils';

interface Props {
  text: string;
  label: string;
  className?: string;
}

export function CopyBtn({ text, label, className }: Props): React.ReactElement {
  const [copied, setCopied] = useState(false);
  const copy = async () => {
    const ok = await copyToClipboard(text);
    if (ok) {
      setCopied(true);
      setTimeout(() => setCopied(false), 1200);
    } else {
      setCopied(false);
    }
  };
  return (
    <button
      onClick={copy}
      className={className ?? 'rounded border px-2 py-1 text-label'}
      style={
        className ? undefined : { borderColor: 'var(--color-border)', color: 'var(--color-text-2)' }
      }
      title={text}
    >
      {copied ? 'ok' : label}
    </button>
  );
}
