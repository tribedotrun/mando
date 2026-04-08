import React, { useState } from 'react';
import { Copy, Check } from 'lucide-react';
import { copyToClipboard } from '#renderer/utils';
import { Button } from '#renderer/components/ui/button';
import { Tooltip, TooltipTrigger, TooltipContent } from '#renderer/components/ui/tooltip';

const COPY_FEEDBACK_MS = 1200;

interface Props {
  text: string;
  label: string;
  className?: string;
}

export function CopyBtn({ text, label, className }: Props): React.ReactElement {
  const [copied, setCopied] = useState(false);
  const [copying, setCopying] = useState(false);
  const copy = () => {
    setCopying(true);
    void copyToClipboard(text)
      .then((ok) => {
        if (ok) {
          setCopied(true);
          setTimeout(() => setCopied(false), COPY_FEEDBACK_MS);
        } else {
          setCopied(false);
        }
      })
      .finally(() => setCopying(false));
  };
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button
          variant="ghost"
          size="icon-xs"
          onClick={copy}
          disabled={copying}
          className={className}
        >
          <span className="transition-fade">
            {copied ? <Check size={12} className="text-success" /> : <Copy size={12} />}
          </span>
          <span className="sr-only">{copied ? 'Copied' : label}</span>
        </Button>
      </TooltipTrigger>
      <TooltipContent>{copied ? 'Copied!' : label}</TooltipContent>
    </Tooltip>
  );
}
