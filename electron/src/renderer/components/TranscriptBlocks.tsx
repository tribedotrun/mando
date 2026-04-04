import React, { useState } from 'react';
import { PrMarkdown } from '#renderer/components/PrMarkdown';

// ── Types (shared with TranscriptViewer) ──

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

// ── Timestamp helpers ──

function localizeTimestamp(ts: string): string {
  const d = new Date(ts);
  if (Number.isNaN(d.getTime())) return ts;
  return d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' });
}

function localizeMeta(meta: string): string {
  return meta.replace(/\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?Z?/g, (m) =>
    localizeTimestamp(m),
  );
}

// ── Tool colors ──

const TOOL_COLORS: Record<string, string> = {
  Bash: 'var(--color-success)',
  Read: 'var(--color-accent)',
  Edit: 'var(--color-stale)',
  Write: 'var(--color-stale)',
  Grep: 'var(--color-text-2)',
  Glob: 'var(--color-text-2)',
  Agent: '#e879a0',
  Skill: 'var(--color-accent)',
  Error: 'var(--color-error)',
  StructuredOutput: 'var(--color-success)',
};

// ── Components ──

function cleanLabel(label: string): string {
  return label.replace(/\*([^*]+)\*/g, '$1').replace(/`([^`]+)`/g, '$1');
}

function ToolBlock({
  block,
}: {
  block: Extract<ContentBlock, { type: 'tool' }>;
}): React.ReactElement {
  const [open, setOpen] = useState(false);
  const color = TOOL_COLORS[block.name] || 'var(--color-text-2)';
  const hasBody = block.body.trim().length > 0;

  return (
    <div
      className="my-1 rounded"
      style={{
        background: 'var(--color-surface-2)',
        border: '1px solid var(--color-border-subtle)',
      }}
    >
      <button
        className="flex w-full items-center gap-2 border-none bg-transparent px-3 py-1.5 text-left text-label"
        style={{ color: 'var(--color-text-1)', cursor: hasBody ? 'pointer' : 'default' }}
        onClick={() => hasBody && setOpen((v) => !v)}
      >
        <span
          className="inline-block h-1.5 w-1.5 shrink-0 rounded-full"
          style={{ background: color }}
        />
        <span className="font-semibold" style={{ color }}>
          {block.name}
        </span>
        {block.label && (
          <span
            className="min-w-0 truncate text-label"
            style={{ color: 'var(--color-text-3)', textTransform: 'none' }}
          >
            {cleanLabel(block.label)}
          </span>
        )}
        {hasBody && (
          <span className="ml-auto" style={{ color: 'var(--color-text-4)' }}>
            {open ? '\u25BC' : '\u25B6'}
          </span>
        )}
      </button>
      {open && hasBody && (
        <div className="border-t px-3 py-2" style={{ borderColor: 'var(--color-border-subtle)' }}>
          <pre
            className="overflow-x-auto whitespace-pre-wrap text-code leading-relaxed"
            style={{ color: 'var(--color-text-1)' }}
          >
            {block.lang === 'diff' ? <DiffContent text={block.body} /> : block.body}
          </pre>
        </div>
      )}
    </div>
  );
}

function DiffContent({ text }: { text: string }): React.ReactElement {
  return (
    <>
      {text.split('\n').map((line, i) => {
        let color = 'var(--color-text-2)';
        let bg = 'transparent';
        if (line.startsWith('+ ')) {
          color = 'var(--color-success)';
          bg = 'color-mix(in srgb, var(--color-success) 8%, transparent)';
        } else if (line.startsWith('- ')) {
          color = 'var(--color-error)';
          bg = 'color-mix(in srgb, var(--color-error) 8%, transparent)';
        }
        return (
          <div key={i} style={{ color, background: bg, paddingInline: 4, marginInline: -4 }}>
            {line}
          </div>
        );
      })}
    </>
  );
}

function SectionHeader({ section }: { section: TranscriptSection }): React.ReactElement {
  const isHuman = section.kind === 'human';
  const dotColor = isHuman ? 'var(--color-needs-human)' : 'var(--color-accent)';
  const label = isHuman ? section.heading : section.heading.replace(/^Turn/, 'Turn');

  return (
    <div className="flex items-center gap-2 pt-3 pb-1">
      <span
        className="inline-block h-2 w-2 shrink-0 rounded-full"
        style={{ background: dotColor }}
      />
      <span className="text-label font-semibold" style={{ color: dotColor }}>
        {label}
      </span>
      {section.meta && (
        <span
          className="text-label"
          style={{
            color: 'var(--color-text-4)',
            fontFamily: 'var(--font-mono)',
            textTransform: 'none',
          }}
        >
          {localizeMeta(section.meta)}
        </span>
      )}
    </div>
  );
}

export function SectionRow({ section }: { section: TranscriptSection }): React.ReactElement {
  const [showTools, setShowTools] = useState(false);
  const textBlocks = section.blocks.filter((b) => b.type === 'text' && b.content);
  const toolBlocks = section.blocks.filter((b) => b.type === 'tool' || b.type === 'results');
  const toolCount = toolBlocks.filter((b) => b.type === 'tool').length;

  return (
    <div>
      <SectionHeader section={section} />
      {textBlocks.map((block, bi) => (
        <div
          key={bi}
          className="py-0.5 text-caption leading-relaxed"
          style={{ color: 'var(--color-text-1)' }}
        >
          <PrMarkdown text={(block as Extract<ContentBlock, { type: 'text' }>).content} />
        </div>
      ))}
      {toolCount > 0 && (
        <>
          <button
            className="mt-1 flex items-center gap-1.5 rounded border-none bg-transparent px-0 py-0.5 text-label"
            style={{ color: 'var(--color-text-4)', cursor: 'pointer' }}
            onClick={() => setShowTools((v) => !v)}
          >
            <span style={{ fontSize: 8 }}>{showTools ? '\u25BC' : '\u25B6'}</span>
            {toolCount} tool call{toolCount !== 1 ? 's' : ''}
          </button>
          {showTools &&
            toolBlocks.map((block, bi) => {
              if (block.type === 'tool') {
                return <ToolBlock key={bi} block={block} />;
              }
              if (block.type === 'results') {
                return (
                  <div
                    key={bi}
                    className="py-0.5 text-label italic"
                    style={{ color: 'var(--color-text-4)' }}
                  >
                    {block.content}
                  </div>
                );
              }
              return null;
            })}
        </>
      )}
    </div>
  );
}
