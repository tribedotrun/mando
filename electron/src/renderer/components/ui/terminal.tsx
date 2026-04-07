import React, { useRef, useState } from 'react';
import Ansi from 'ansi-to-react';
import { Check, Copy } from 'lucide-react';
import { cn } from '#renderer/cn';

interface TerminalProps {
  output: string;
  className?: string;
}

export function Terminal({ output, className }: TerminalProps): React.ReactElement {
  const [copied, setCopied] = useState(false);
  const timerRef = useRef<ReturnType<typeof setTimeout>>(undefined);

  const handleCopy = async () => {
    await navigator.clipboard.writeText(output);
    setCopied(true);
    clearTimeout(timerRef.current);
    timerRef.current = setTimeout(() => setCopied(false), 1500);
  };

  return (
    <div
      data-slot="terminal"
      className={cn('group/terminal relative my-2 rounded-md bg-muted', className)}
    >
      {/* Header: copy button -- visible on hover via group */}
      <div className="flex items-center justify-end px-3 pt-1.5 pb-0 opacity-0 transition-opacity group-hover/terminal:opacity-100">
        <span className="text-[10px] uppercase tracking-wider text-text-3">terminal</span>
        <button
          type="button"
          onClick={handleCopy}
          className="ml-2 flex items-center gap-1 rounded px-1.5 py-0.5 text-[10px] text-text-3 transition-colors hover:bg-secondary hover:text-muted-foreground"
        >
          {copied ? <Check size={10} /> : <Copy size={10} />}
        </button>
      </div>

      {/* Terminal output */}
      <div className="overflow-x-auto px-3 pb-3 pt-1 font-mono text-[11px] leading-relaxed">
        <pre className="whitespace-pre-wrap text-foreground">
          <Ansi>{output}</Ansi>
        </pre>
      </div>
    </div>
  );
}
