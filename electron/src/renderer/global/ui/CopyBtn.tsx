import React, { useState } from 'react';
import { Copy, Check } from 'lucide-react';
import { copyToClipboard } from '#renderer/global/runtime/useFeedback';
import { useCopyFeedback } from '#renderer/global/runtime/useCopyFeedback';
import { Button } from '#renderer/global/ui/button';
import { Tooltip, TooltipTrigger, TooltipContent } from '#renderer/global/ui/tooltip';

interface Props {
  text: string;
  label: string;
  className?: string;
}

export function CopyBtn({ text, label, className }: Props): React.ReactElement {
  const { copied, markCopied } = useCopyFeedback();
  const [copying, setCopying] = useState(false);
  const copy = () => {
    setCopying(true);
    void copyToClipboard(text)
      .then((ok) => {
        if (ok) markCopied();
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
