import React, { useState } from 'react';
import { Copy, Check } from 'lucide-react';
import { copyToClipboard } from '#renderer/utils';
import { Button } from '#renderer/components/ui/button';
import { Tooltip, TooltipTrigger, TooltipContent } from '#renderer/components/ui/tooltip';

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
    <Tooltip>
      <TooltipTrigger asChild>
        <Button variant="ghost" size="icon-xs" onClick={copy} className={className}>
          {copied ? <Check size={12} /> : <Copy size={12} />}
          <span className="sr-only">{copied ? 'Copied' : label}</span>
        </Button>
      </TooltipTrigger>
      <TooltipContent>{copied ? 'Copied!' : label}</TooltipContent>
    </Tooltip>
  );
}
