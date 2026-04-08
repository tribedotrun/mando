import React, { useState } from 'react';
import { ChevronDown, ChevronRight } from 'lucide-react';
import { localizeMeta } from '#renderer/utils';
import { PrMarkdown } from '#renderer/domains/captain';
import { Badge } from '#renderer/components/ui/badge';
import { Button } from '#renderer/components/ui/button';
import { CodeBlock } from '#renderer/components/ui/code-block';
import { Terminal } from '#renderer/components/ui/terminal';
import {
  Collapsible,
  CollapsibleTrigger,
  CollapsibleContent,
} from '#renderer/components/ui/collapsible';

// -- Types (shared with TranscriptViewer) --

export interface TranscriptSection {
  kind: 'human' | 'turn' | 'queue-op' | 'session-end';
  heading: string;
  meta?: string;
  blocks: ContentBlock[];
}

export type ContentBlock =
  | { type: 'text'; content: string }
  | { type: 'tool'; name: string; label: string; body: string; lang?: string }
  | { type: 'results'; content: string };

// -- Tool colors --

const TOOL_COLORS: Record<string, string> = {
  Bash: 'var(--success)',
  Read: 'var(--muted-foreground)',
  Edit: 'var(--stale)',
  Write: 'var(--stale)',
  Grep: 'var(--muted-foreground)',
  Glob: 'var(--muted-foreground)',
  Agent: '#e879a0',
  Skill: 'var(--muted-foreground)',
  Error: 'var(--destructive)',
  StructuredOutput: 'var(--success)',
};

// -- Helpers --

function cleanLabel(label: string): string {
  return label.replace(/\*([^*]+)\*/g, '$1').replace(/`([^`]+)`/g, '$1');
}

/** Detect if text looks like terminal output (ANSI codes or shell-prompt lines). */
function looksLikeTerminalOutput(text: string): boolean {
  // eslint-disable-next-line no-control-regex
  return /\x1b\[/.test(text) || /^\$\s/.test(text);
}

/** Map tool lang field to a Shiki-compatible language key. */
function toolLangToShiki(name: string, lang?: string): string {
  if (lang) return lang;
  const toolLangs: Record<string, string> = {
    Bash: 'bash',
    Read: 'text',
    Grep: 'text',
    Glob: 'text',
    Edit: 'diff',
    Write: 'text',
  };
  return toolLangs[name] ?? 'text';
}

// -- Components --

function ToolBlockBody({
  block,
}: {
  block: Extract<ContentBlock, { type: 'tool' }>;
}): React.ReactElement {
  const lang = toolLangToShiki(block.name, block.lang);

  // Diff gets its own renderer (handles +/- coloring well)
  if (lang === 'diff') {
    return <DiffContent text={block.body} />;
  }

  // Bash output with ANSI codes goes through Terminal
  if (block.name === 'Bash' && looksLikeTerminalOutput(block.body)) {
    return <Terminal output={block.body} className="my-0" />;
  }

  // Everything else gets syntax highlighting
  return <CodeBlock code={block.body} language={lang} className="my-0" />;
}

function ToolBlock({
  block,
}: {
  block: Extract<ContentBlock, { type: 'tool' }>;
}): React.ReactElement {
  const [open, setOpen] = useState(false);
  const color = TOOL_COLORS[block.name] || 'var(--muted-foreground)';
  const hasBody = block.body.trim().length > 0;

  return (
    <Collapsible open={open} onOpenChange={setOpen} className="my-1">
      <div className="rounded-md bg-muted">
        <CollapsibleTrigger asChild>
          <Button
            variant="ghost"
            className="flex h-auto w-full items-center gap-2 rounded-none px-3 py-1.5 text-left text-label text-foreground"
            style={{ cursor: hasBody ? 'pointer' : 'default' }}
            onClick={(e) => {
              if (!hasBody) e.preventDefault();
            }}
          >
            <span
              className="inline-block h-1.5 w-1.5 shrink-0 rounded-full"
              style={{ background: color }}
            />
            <span className="font-semibold" style={{ color }}>
              {block.name}
            </span>
            {block.label && (
              <span className="min-w-0 truncate text-label text-muted-foreground normal-case">
                {cleanLabel(block.label)}
              </span>
            )}
            {hasBody && (
              <span className="ml-auto text-muted-foreground">
                {open ? <ChevronDown size={12} /> : <ChevronRight size={12} />}
              </span>
            )}
          </Button>
        </CollapsibleTrigger>
        <CollapsibleContent>
          {hasBody && (
            <div className="px-1 pb-2">
              <ToolBlockBody block={block} />
            </div>
          )}
        </CollapsibleContent>
      </div>
    </Collapsible>
  );
}

export function StructuredOutputBlock({
  block,
}: {
  block: Extract<ContentBlock, { type: 'tool' }>;
}): React.ReactElement {
  const parsed = (() => {
    try {
      return JSON.parse(block.body) as Record<string, unknown>;
    } catch {
      return null;
    }
  })();

  const header = (
    <div className="mb-1 flex items-center gap-2">
      <span className="inline-block h-1.5 w-1.5 shrink-0 rounded-full bg-success" />
      <Badge variant="secondary" className="text-[10px] font-semibold text-success">
        STRUCTURED OUTPUT
      </Badge>
    </div>
  );

  if (!parsed) {
    return (
      <div className="my-2 rounded-md bg-muted px-3 py-2">
        {header}
        <CodeBlock code={block.body} language="json" className="my-0" />
      </div>
    );
  }

  return (
    <div className="my-2 min-w-0 space-y-2 overflow-hidden rounded-md bg-muted px-3 py-2">
      {header}
      {Object.entries(parsed).map(([key, value]) => {
        const text = typeof value === 'string' ? value : JSON.stringify(value, null, 2);
        return (
          <div key={key} className="min-w-0">
            <div className="text-label mb-0.5 font-semibold text-muted-foreground">{key}</div>
            <div className="min-w-0 text-caption leading-relaxed text-foreground [overflow-wrap:anywhere]">
              <PrMarkdown text={text} />
            </div>
          </div>
        );
      })}
    </div>
  );
}

function DiffContent({ text }: { text: string }): React.ReactElement {
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

function SectionHeader({ section }: { section: TranscriptSection }): React.ReactElement {
  const isHuman = section.kind === 'human';

  return (
    <div className="flex items-center gap-2 pb-1 pt-3">
      <Badge variant={isHuman ? 'destructive' : 'default'} className="text-[10px]">
        {isHuman ? section.heading : section.heading.replace(/^Turn/, 'Turn')}
      </Badge>
      {section.meta && (
        <span className="text-label font-mono normal-case text-muted-foreground">
          {localizeMeta(section.meta)}
        </span>
      )}
    </div>
  );
}

function HumanSection({ section }: { section: TranscriptSection }): React.ReactElement {
  const textBlocks = section.blocks.filter((b) => b.type === 'text' && b.content);

  return (
    <div>
      <SectionHeader section={section} />
      <div
        className="rounded-md py-2 pl-3 pr-3"
        style={{
          borderLeft: '2px solid var(--needs-human)',
          background: 'var(--needs-human-bg)',
        }}
      >
        {textBlocks.map((block, bi) => (
          <div key={bi} className="text-caption leading-relaxed text-foreground">
            <PrMarkdown text={(block as Extract<ContentBlock, { type: 'text' }>).content} />
          </div>
        ))}
      </div>
    </div>
  );
}

export function SectionRow({ section }: { section: TranscriptSection }): React.ReactElement {
  const [showTools, setShowTools] = useState(false);

  // Human messages get distinct visual treatment
  if (section.kind === 'human') {
    return <HumanSection section={section} />;
  }
  const textBlocks = section.blocks.filter((b) => b.type === 'text' && b.content);
  // Separate StructuredOutput from regular tool calls -- it's surfaced prominently.
  const structuredBlocks = section.blocks.filter(
    (b) => b.type === 'tool' && b.name === 'StructuredOutput',
  ) as Extract<ContentBlock, { type: 'tool' }>[];
  const toolBlocks = section.blocks.filter(
    (b) => (b.type === 'tool' && b.name !== 'StructuredOutput') || b.type === 'results',
  );
  const toolCount = toolBlocks.filter((b) => b.type === 'tool').length;

  return (
    <div>
      <SectionHeader section={section} />
      {structuredBlocks.map((block, bi) => (
        <StructuredOutputBlock key={`so-${bi}`} block={block} />
      ))}
      {textBlocks.map((block, bi) => (
        <div key={bi} className="py-1 text-caption leading-relaxed text-foreground">
          <PrMarkdown text={(block as Extract<ContentBlock, { type: 'text' }>).content} />
        </div>
      ))}
      {toolCount > 0 && (
        <Collapsible open={showTools} onOpenChange={setShowTools}>
          <CollapsibleTrigger asChild>
            <Button variant="ghost" size="xs" className="mt-1 gap-1.5 text-muted-foreground">
              {showTools ? <ChevronDown size={11} /> : <ChevronRight size={11} />}
              <span>
                {toolCount} tool call{toolCount !== 1 ? 's' : ''}
              </span>
            </Button>
          </CollapsibleTrigger>
          <CollapsibleContent>
            {toolBlocks.map((block, bi) => {
              if (block.type === 'tool') {
                return <ToolBlock key={bi} block={block} />;
              }
              if (block.type === 'results') {
                return (
                  <div key={bi} className="py-1 text-label italic text-muted-foreground">
                    {block.content}
                  </div>
                );
              }
              return null;
            })}
          </CollapsibleContent>
        </Collapsible>
      )}
    </div>
  );
}
