import React, { useState } from 'react';

interface Props {
  text: string;
  label: string;
  className?: string;
}

export function CopyBtn({ text, label, className }: Props): React.ReactElement {
  const [copied, setCopied] = useState(false);
  const copy = () => {
    navigator.clipboard.writeText(text).then(
      () => {
        setCopied(true);
        setTimeout(() => setCopied(false), 1200);
      },
      () => {
        setCopied(false);
        console.warn('Clipboard write failed — access denied or unavailable');
      },
    );
  };
  return (
    <button
      onClick={copy}
      className={className ?? 'rounded border px-1.5 py-0.5 text-[0.6rem]'}
      style={
        className ? undefined : { borderColor: 'var(--color-border)', color: 'var(--color-text-2)' }
      }
      title={text}
    >
      {copied ? 'ok' : label}
    </button>
  );
}
