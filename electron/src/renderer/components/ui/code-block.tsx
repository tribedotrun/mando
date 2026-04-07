import React, { useRef, useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { Check, Copy } from 'lucide-react';
import { cn } from '#renderer/cn';

type Highlighter = {
  codeToHtml: (code: string, opts: { lang: string; theme: string }) => string;
};

// Singleton highlighter -- loaded once, reused everywhere
let highlighterPromise: Promise<Highlighter> | null = null;

function getHighlighter(): Promise<Highlighter> {
  if (!highlighterPromise) {
    const p = import('shiki/bundle/full').then((mod) =>
      mod.createHighlighter({
        themes: ['github-dark-default'],
        langs: [
          'typescript',
          'javascript',
          'json',
          'bash',
          'python',
          'rust',
          'diff',
          'markdown',
          'yaml',
          'toml',
          'html',
          'css',
          'sql',
          'tsx',
          'jsx',
        ],
      }),
    );
    p.catch(() => {
      highlighterPromise = null;
    });
    highlighterPromise = p;
  }
  return highlighterPromise;
}

async function highlight(code: string, lang: string): Promise<string | null> {
  const hl = await getHighlighter();
  try {
    return hl.codeToHtml(code, { lang, theme: 'github-dark-default' });
  } catch {
    return null;
  }
}

// Map common language aliases to what shiki expects
const LANG_ALIASES: Record<string, string> = {
  ts: 'typescript',
  js: 'javascript',
  py: 'python',
  sh: 'bash',
  shell: 'bash',
  yml: 'yaml',
  md: 'markdown',
  rs: 'rust',
};

function normalizeLang(lang: string): string {
  return LANG_ALIASES[lang] ?? lang;
}

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
  const [copied, setCopied] = useState(false);
  const timerRef = useRef<ReturnType<typeof setTimeout>>(undefined);
  const resolvedLang = normalizeLang(language);

  const { data: html } = useQuery({
    queryKey: ['shiki-highlight', resolvedLang, code],
    queryFn: () => highlight(code, resolvedLang),
    staleTime: Infinity,
    gcTime: 5 * 60 * 1000,
  });

  const handleCopy = async () => {
    await navigator.clipboard.writeText(code);
    setCopied(true);
    clearTimeout(timerRef.current);
    timerRef.current = setTimeout(() => setCopied(false), 1500);
  };

  return (
    <div
      data-slot="code-block"
      className={cn('group/code relative my-2 rounded-md bg-muted', className)}
    >
      {/* Header: language + copy -- visible on hover via group */}
      <div className="flex items-center justify-end gap-2 px-3 pt-1.5 pb-0 opacity-0 transition-opacity group-hover/code:opacity-100">
        {resolvedLang !== 'text' && (
          <span className="text-[10px] uppercase tracking-wider text-text-3">{resolvedLang}</span>
        )}
        <button
          type="button"
          onClick={handleCopy}
          className="flex items-center gap-1 rounded px-1.5 py-0.5 text-[10px] text-text-3 transition-colors hover:bg-secondary hover:text-muted-foreground"
        >
          {copied ? <Check size={10} /> : <Copy size={10} />}
        </button>
      </div>

      {/* Code content */}
      <div className="overflow-x-auto px-3 pb-3 pt-1 font-mono text-[11px] leading-relaxed">
        {html ? (
          <div
            className="shiki-wrapper [&_.shiki]:!bg-transparent [&_pre]:!bg-transparent [&_code]:!bg-transparent"
            dangerouslySetInnerHTML={{ __html: html }}
          />
        ) : (
          <pre className="whitespace-pre-wrap text-foreground">{code}</pre>
        )}
      </div>
    </div>
  );
}
