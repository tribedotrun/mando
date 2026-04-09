import React, { useState } from 'react';
import { ChevronDown, ChevronRight } from 'lucide-react';
import { localizeMeta } from '#renderer/utils';
import { PrMarkdown } from '#renderer/domains/captain';
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
  | { type: 'results'; content: string }
  | { type: 'error'; body: string };

export interface PromptGroup {
  prompt: TranscriptSection | null;
  turns: TranscriptSection[];
}

// -- Helpers --

function cleanLabel(label: string): string {
  return label.replace(/\*([^*]+)\*/g, '$1').replace(/`([^`]+)`/g, '$1');
}

function looksLikeTerminalOutput(text: string): boolean {
  // eslint-disable-next-line no-control-regex
  return /\x1b\[/.test(text) || /^\$\s/.test(text);
}

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
  if (lang === 'diff') return <DiffContent text={block.body} />;
  if (block.name === 'Bash' && looksLikeTerminalOutput(block.body)) {
    return <Terminal output={block.body} className="my-0" />;
  }
  return <CodeBlock code={block.body} language={lang} className="my-0" />;
}

function ErrorBlock({
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

function ToolBlock({
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

  if (!parsed) {
    return (
      <div className="my-2 rounded-md bg-muted px-3 py-2">
        <div className="mb-1 text-label font-medium text-muted-foreground">Structured Output</div>
        <CodeBlock code={block.body} language="json" className="my-0" />
      </div>
    );
  }

  return (
    <div className="my-2 min-w-0 space-y-2 overflow-hidden rounded-md bg-muted px-3 py-2">
      <div className="text-label font-medium text-muted-foreground">Structured Output</div>
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
  return (
    <div className="flex items-center gap-2 pb-1 pt-3 text-label text-muted-foreground">
      <span className="font-medium">{section.heading}</span>
      {section.meta && <span className="font-mono normal-case">{localizeMeta(section.meta)}</span>}
    </div>
  );
}

/** Extract skill name from prompt body that starts with "Base directory for this skill: .../name" */
function detectSkill(content: string): string | null {
  const m = content.match(/^Base directory for this skill: .+\/([^/\s]+)/);
  return m ? m[1] : null;
}

function HumanSection({ section }: { section: TranscriptSection }): React.ReactElement {
  const textBlocks = section.blocks.filter((b) => b.type === 'text' && b.content);
  const isCommand = section.heading.startsWith('/');

  const totalContent = textBlocks
    .map((b) => (b as Extract<ContentBlock, { type: 'text' }>).content)
    .join('\n');
  const skillName = detectSkill(totalContent);
  const [open, setOpen] = useState(false);

  if (skillName) {
    return (
      <div className="mt-4 rounded-lg bg-muted/40 px-3.5 py-3">
        <div className="flex items-center gap-2 text-label">
          <span className="font-medium text-foreground">{section.heading}</span>
          {section.meta && (
            <span className="font-mono normal-case text-muted-foreground">
              {localizeMeta(section.meta)}
            </span>
          )}
        </div>
        <Collapsible open={open} onOpenChange={setOpen}>
          <CollapsibleTrigger asChild>
            <button className="mt-1.5 flex items-center gap-1.5 rounded px-1 py-0.5 text-label hover:bg-muted">
              {open ? <ChevronDown size={11} /> : <ChevronRight size={11} />}
              <span className="font-mono font-medium text-muted-foreground">/{skillName}</span>
            </button>
          </CollapsibleTrigger>
          <CollapsibleContent>
            {textBlocks.map((block, bi) => (
              <div key={bi} className="text-caption leading-relaxed text-foreground">
                <PrMarkdown text={(block as Extract<ContentBlock, { type: 'text' }>).content} />
              </div>
            ))}
          </CollapsibleContent>
        </Collapsible>
      </div>
    );
  }

  return (
    <div className="mt-4 rounded-lg bg-muted/40 px-3.5 py-3">
      <div className="flex items-center gap-2 pb-1.5 text-label">
        <span
          className={
            isCommand
              ? 'font-mono font-medium normal-case text-muted-foreground'
              : 'font-medium text-foreground'
          }
        >
          {section.heading}
        </span>
        {section.meta && (
          <span className="font-mono normal-case text-muted-foreground">
            {localizeMeta(section.meta)}
          </span>
        )}
      </div>
      {textBlocks.map((block, bi) => (
        <div key={bi} className="text-caption leading-relaxed text-foreground">
          <PrMarkdown text={(block as Extract<ContentBlock, { type: 'text' }>).content} />
        </div>
      ))}
    </div>
  );
}

function TurnContent({ section }: { section: TranscriptSection }): React.ReactElement {
  const [showTools, setShowTools] = useState(false);
  const textBlocks = section.blocks.filter((b) => b.type === 'text' && b.content);
  const toolBlocks = section.blocks.filter(
    (b) =>
      (b.type === 'tool' && b.name !== 'StructuredOutput') ||
      b.type === 'results' ||
      b.type === 'error',
  );
  const toolCount = toolBlocks.filter((b) => b.type === 'tool').length;

  return (
    <>
      {toolBlocks.length > 0 && (
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
              if (block.type === 'tool') return <ToolBlock key={bi} block={block} />;
              if (block.type === 'error')
                return (
                  <ErrorBlock key={bi} block={block as Extract<ContentBlock, { type: 'error' }>} />
                );
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
      {textBlocks.map((block, bi) => (
        <div key={`t-${bi}`} className="py-1 text-caption leading-relaxed text-foreground">
          <PrMarkdown text={(block as Extract<ContentBlock, { type: 'text' }>).content} />
        </div>
      ))}
    </>
  );
}

export function SectionRow({ section }: { section: TranscriptSection }): React.ReactElement {
  if (section.kind === 'human') return <HumanSection section={section} />;
  return (
    <div>
      <SectionHeader section={section} />
      <TurnContent section={section} />
    </div>
  );
}

export function PromptGroupRow({ group }: { group: PromptGroup }): React.ReactElement {
  const lastTurn = group.turns.length > 0 ? group.turns[group.turns.length - 1] : null;
  const intermediateTurns = group.turns.slice(0, -1);
  const [showIntermediate, setShowIntermediate] = useState(false);

  const intermediateToolCount = intermediateTurns.reduce(
    (sum, t) =>
      sum + t.blocks.filter((b) => b.type === 'tool' && b.name !== 'StructuredOutput').length,
    0,
  );

  return (
    <div className="py-3">
      {group.prompt &&
        (group.prompt.kind === 'session-end' ? (
          <div className="pt-4 pb-1 text-label italic text-muted-foreground">
            {group.prompt.heading.replace(/^\*|\*$/g, '')}
          </div>
        ) : (
          <HumanSection section={group.prompt} />
        ))}

      {intermediateTurns.length > 0 && (
        <Collapsible open={showIntermediate} onOpenChange={setShowIntermediate}>
          <CollapsibleTrigger asChild>
            <Button variant="ghost" size="xs" className="my-2 gap-1.5 text-muted-foreground">
              {showIntermediate ? <ChevronDown size={11} /> : <ChevronRight size={11} />}
              <span>
                {intermediateTurns.length} turn{intermediateTurns.length !== 1 ? 's' : ''}
                {intermediateToolCount > 0 &&
                  `, ${intermediateToolCount} tool call${intermediateToolCount !== 1 ? 's' : ''}`}
              </span>
            </Button>
          </CollapsibleTrigger>
          <CollapsibleContent>
            {intermediateTurns.map((turn, ti) => (
              <SectionRow key={ti} section={turn} />
            ))}
          </CollapsibleContent>
        </Collapsible>
      )}

      {lastTurn && <TurnContent section={lastTurn} />}
    </div>
  );
}
