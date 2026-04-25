import React from 'react';
import { Check, Copy } from 'lucide-react';
import { useHighlight } from '#renderer/global/runtime/useHighlight';
import { useCopyFeedback } from '#renderer/global/runtime/useCopyFeedback';
import { cn } from '#renderer/global/service/cn';
import { normalizeLang } from '#renderer/global/service/codeBlockHelpers';
import { copyToClipboard } from '#renderer/global/runtime/useFeedback';

interface CodeBlockProps {
  code: string;
  language?: string;
  className?: string;
}

export function CodeBlock({
  code,
  language = 'text',
  className,
}: CodeBlockProps): React.ReactElement {
  const { copied, markCopied } = useCopyFeedback(1500);
  const resolvedLang = normalizeLang(language);

  const { data: html } = useHighlight(code, resolvedLang);

  const handleCopy = () => {
    void (async () => {
      const ok = await copyToClipboard(code);
      if (ok) markCopied();
    })();
  };

  return (
    <div
      data-slot="code-block"
      className={cn('group/code relative my-2 rounded-md bg-muted', className)}
    >
      {/* Header: language + copy -- visible on hover via group */}
      <div className="flex items-center justify-end gap-2 px-3 pt-1.5 pb-0 opacity-0 transition-opacity group-hover/code:opacity-100">
        {resolvedLang !== 'text' && (
          <span className="text-[11px] uppercase tracking-wider text-text-3">{resolvedLang}</span>
        )}
        <button
          type="button"
          onClick={handleCopy}
          className="flex items-center gap-1 rounded px-1.5 py-0.5 text-[11px] text-text-3 transition-colors hover:bg-secondary hover:text-muted-foreground"
        >
          {copied ? <Check size={10} /> : <Copy size={10} />}
        </button>
      </div>

      {/* Code content */}
      <div className="min-w-0 overflow-x-auto px-3 pb-3 pt-1 font-mono text-[11px] leading-relaxed">
        {html ? (
          <div
            className="shiki-wrapper [&_.shiki]:!bg-transparent [&_pre]:!bg-transparent [&_code]:!bg-transparent"
            dangerouslySetInnerHTML={{ __html: html }}
          />
        ) : (
          <pre className="whitespace-pre text-foreground">{code}</pre>
        )}
      </div>
    </div>
  );
}
