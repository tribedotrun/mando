import React, { useState } from 'react';
import { ChevronDown, ChevronRight } from 'lucide-react';
import { CodeBlock } from '#renderer/global/ui/code-block';
import { Terminal } from '#renderer/global/ui/terminal';
import {
  Collapsible,
  CollapsibleTrigger,
  CollapsibleContent,
} from '#renderer/global/ui/collapsible';
import {
  cleanLabel,
  looksLikeTerminalOutput,
  toolLangToShiki,
  type ContentBlock,
} from '#renderer/domains/sessions/service/transcript';

export function DiffContent({ text }: { text: string }): React.ReactElement {
  return (
    <pre className="overflow-x-auto whitespace-pre-wrap px-3 py-2 font-mono text-[11px] leading-relaxed">
      {text.split('\n').map((line, i) => {
        let color = 'var(--muted-foreground)';
        let bg = 'transparent';
        if (line.startsWith('+ ')) {
          color = 'var(--success)';
          bg = 'color-mix(in srgb, var(--success) 8%, transparent)';
        } else if (line.startsWith('- ')) {
          color = 'var(--destructive)';
          bg = 'color-mix(in srgb, var(--destructive) 8%, transparent)';
        }
        return (
          <div key={i} style={{ color, background: bg, paddingInline: 4, marginInline: -4 }}>
            {line}
          </div>
        );
      })}
    </pre>
  );
}

export function ToolBlockBody({
  block,
}: {
  block: Extract<ContentBlock, { type: 'tool' }>;
}): React.ReactElement {
  const lang = toolLangToShiki(block.name, block.lang);
  if (lang === 'diff') return <DiffContent text={block.body} />;
  if (block.name === 'Bash' && looksLikeTerminalOutput(block.body)) {
    return <Terminal output={block.body} className="my-0" />;
  }
  return <CodeBlock code={block.body} language={lang} className="my-0" />;
}

export function ErrorBlock({
  block,
}: {
  block: Extract<ContentBlock, { type: 'error' }>;
}): React.ReactElement {
  const [open, setOpen] = useState(false);
  const hasBody = block.body.trim().length > 0;

  return (
    <Collapsible open={open} onOpenChange={setOpen} className="my-0.5">
      <CollapsibleTrigger asChild>
        <button
          className={`flex w-full items-center gap-1.5 rounded px-2 py-1 text-left text-label text-destructive/80${hasBody ? ' hover:bg-muted' : ''}`}
          style={{ cursor: hasBody ? 'pointer' : 'default' }}
          onClick={(e) => {
            if (!hasBody) e.preventDefault();
          }}
        >
          <span className="font-medium">Error</span>
          {hasBody && (
            <span className="ml-auto">
              {open ? <ChevronDown size={11} /> : <ChevronRight size={11} />}
            </span>
          )}
        </button>
      </CollapsibleTrigger>
      <CollapsibleContent>
        {hasBody && (
          <div className="pb-2 pl-2">
            <pre className="whitespace-pre-wrap text-[11px] leading-relaxed text-muted-foreground">
              {block.body}
            </pre>
          </div>
        )}
      </CollapsibleContent>
    </Collapsible>
  );
}

export function ToolBlock({
  block,
}: {
  block: Extract<ContentBlock, { type: 'tool' }>;
}): React.ReactElement {
  const [open, setOpen] = useState(false);
  const hasBody = block.body.trim().length > 0;

  return (
    <Collapsible open={open} onOpenChange={setOpen} className="my-0.5">
      <CollapsibleTrigger asChild>
        <button
          className={`flex w-full items-center gap-1.5 rounded px-2 py-1 text-left text-label text-muted-foreground${hasBody ? ' hover:bg-muted' : ''}`}
          style={{ cursor: hasBody ? 'pointer' : 'default' }}
          onClick={(e) => {
            if (!hasBody) e.preventDefault();
          }}
        >
          <span className="font-medium text-foreground">{block.name}</span>
          {block.label && (
            <span className="min-w-0 truncate text-label normal-case opacity-60">
              {cleanLabel(block.label)}
            </span>
          )}
          {hasBody && (
            <span className="ml-auto">
              {open ? <ChevronDown size={11} /> : <ChevronRight size={11} />}
            </span>
          )}
        </button>
      </CollapsibleTrigger>
      <CollapsibleContent>
        {hasBody && (
          <div className="pb-2 pl-2">
            <ToolBlockBody block={block} />
          </div>
        )}
      </CollapsibleContent>
    </Collapsible>
  );
}
