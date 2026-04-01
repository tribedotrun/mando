import React, { useState, useMemo } from 'react';
import type { SessionEntry } from '#renderer/types';
import { CopyBtn } from '#renderer/components/CopyBtn';
import { PrMarkdown } from '#renderer/components/PrMarkdown';
import { sessionTitle, sessionSubtitle } from '#renderer/components/SessionsHelpers';

// ── Types ──

interface TranscriptSection {
  kind: 'human' | 'turn' | 'queue-op' | 'session-end';
  heading: string;
  meta?: string; // model, timestamp
  blocks: ContentBlock[];
}

type ContentBlock =
  | { type: 'text'; content: string }
  | { type: 'tool'; name: string; label: string; body: string; lang?: string }
  | { type: 'results'; content: string };

// ── Timestamp helpers ──

/** Convert an ISO UTC timestamp to local time display. */
function localizeTimestamp(ts: string): string {
  const d = new Date(ts);
  if (Number.isNaN(d.getTime())) return ts;
  return d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' });
}

/** Replace ISO timestamps in meta strings with local time. */
function localizeMeta(meta: string): string {
  return meta.replace(/\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?Z?/g, (m) =>
    localizeTimestamp(m),
  );
}

// ── Parser ──

function parseTranscript(markdown: string): TranscriptSection[] {
  if (!markdown?.trim()) return [];

  // Split on section dividers: `---\n## `
  const raw = markdown.split(/\n---\n## /);
  const sections: TranscriptSection[] = [];

  for (let i = 0; i < raw.length; i++) {
    let chunk = raw[i];
    // First chunk may start with `---\n## ` or just content
    if (i === 0) {
      if (chunk.startsWith('---\n## ')) chunk = chunk.slice(7);
      else if (chunk.trim() === '' || !chunk.includes('##')) continue;
      else if (chunk.startsWith('## ')) chunk = chunk.slice(3);
    }

    const nlIdx = chunk.indexOf('\n');
    const heading = nlIdx >= 0 ? chunk.slice(0, nlIdx).trim() : chunk.trim();
    const body = nlIdx >= 0 ? chunk.slice(nlIdx + 1) : '';

    let kind: TranscriptSection['kind'] = 'turn';
    if (heading.startsWith('Human')) kind = 'human';
    else if (heading.startsWith('[')) kind = 'queue-op';
    else if (heading.startsWith('*Session end')) {
      sections.push({ kind: 'session-end', heading, blocks: [] });
      continue;
    }

    // Extract meta (backtick-wrapped model/timestamp)
    const metaMatch = heading.match(/`([^`]+)`/g);
    const meta = metaMatch?.map((m) => m.replace(/`/g, '')).join('  ') || undefined;

    const blocks =
      kind === 'turn' ? parseTurnBody(body) : [{ type: 'text' as const, content: body.trim() }];

    sections.push({ kind, heading: heading.replace(/`[^`]+`/g, '').trim(), meta, blocks });
  }

  return sections;
}

function parseTurnBody(body: string): ContentBlock[] {
  const blocks: ContentBlock[] = [];
  const lines = body.split('\n');
  let i = 0;
  let textBuf: string[] = [];

  const flushText = () => {
    const txt = textBuf.join('\n').trim();
    if (txt) blocks.push({ type: 'text', content: txt });
    textBuf = [];
  };

  while (i < lines.length) {
    const line = lines[i];

    // Tool block: **ToolName** ...
    const toolMatch = line.match(/^\*\*(\w+)\*\*(.*)$/);
    if (toolMatch) {
      flushText();
      const name = toolMatch[1];
      const label = toolMatch[2].trim();
      // Look for code block following
      let codeBody = '';
      let lang = '';
      if (i + 1 < lines.length && lines[i + 1].startsWith('```')) {
        const fenceLine = lines[i + 1];
        lang = fenceLine.slice(3).trim();
        i += 2;
        const codeLines: string[] = [];
        while (i < lines.length && !lines[i].startsWith('```')) {
          codeLines.push(lines[i]);
          i++;
        }
        codeBody = codeLines.join('\n');
        i++; // skip closing ```
      } else {
        i++;
      }
      blocks.push({ type: 'tool', name, label, body: codeBody, lang });
      continue;
    }

    // Results line: *results: ...*
    if (line.startsWith('*results:') || line.startsWith('*results ')) {
      flushText();
      blocks.push({ type: 'results', content: line.replace(/^\*|\*$/g, '') });
      i++;
      continue;
    }

    // Error block inside results
    if (line.startsWith('**Error:**')) {
      flushText();
      const label = line.replace(/\*\*/g, '').replace(/:$/, '');
      let codeBody = '';
      if (i + 1 < lines.length && lines[i + 1].startsWith('```')) {
        i += 2;
        const codeLines: string[] = [];
        while (i < lines.length && !lines[i].startsWith('```')) {
          codeLines.push(lines[i]);
          i++;
        }
        codeBody = codeLines.join('\n');
        i++; // skip closing ```
      } else {
        i++;
      }
      blocks.push({ type: 'tool', name: label, label: '', body: codeBody });
      continue;
    }

    // Initial context — skip entirely (internal plumbing)
    if (line.startsWith('**Initial context:**')) {
      if (i + 1 < lines.length && lines[i + 1].startsWith('```')) {
        i += 2;
        while (i < lines.length && !lines[i].startsWith('```')) i++;
        i++; // skip closing ```
      } else {
        i++;
      }
      continue;
    }

    textBuf.push(line);
    i++;
  }

  flushText();
  return blocks;
}

// ── Components ──

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

/** Tool names that should start expanded. */
const AUTO_OPEN_TOOLS = new Set(['StructuredOutput']);

function ToolBlock({
  block,
}: {
  block: Extract<ContentBlock, { type: 'tool' }>;
}): React.ReactElement {
  const [open, setOpen] = useState(AUTO_OPEN_TOOLS.has(block.name));
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
        className="flex w-full items-center gap-2 px-3 py-1.5 text-left text-[11px]"
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
          <span className="min-w-0 truncate text-[10px]" style={{ color: 'var(--color-text-3)' }}>
            {cleanLabel(block.label)}
          </span>
        )}
        {hasBody && (
          <span className="ml-auto text-[9px]" style={{ color: 'var(--color-text-4)' }}>
            {open ? '\u25BC' : '\u25B6'}
          </span>
        )}
      </button>
      {open && hasBody && (
        <div className="border-t px-3 py-2" style={{ borderColor: 'var(--color-border-subtle)' }}>
          <pre
            className="overflow-x-auto whitespace-pre-wrap text-[11px] leading-relaxed"
            style={{
              color: 'var(--color-text-1)',
              fontFamily: 'var(--font-mono, Geist Mono, monospace)',
            }}
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

function cleanLabel(label: string): string {
  // Remove markdown formatting from label
  return label.replace(/\*([^*]+)\*/g, '$1').replace(/`([^`]+)`/g, '$1');
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
      <span
        className="text-[11px] font-semibold uppercase tracking-wider"
        style={{ color: dotColor }}
      >
        {label}
      </span>
      {section.meta && (
        <span className="text-[10px] font-mono" style={{ color: 'var(--color-text-4)' }}>
          {localizeMeta(section.meta)}
        </span>
      )}
    </div>
  );
}

// ── Main export ──

interface Props {
  markdown: string;
}

export function TranscriptViewer({ markdown }: Props): React.ReactElement {
  const sections = useMemo(() => parseTranscript(markdown), [markdown]);

  // Filter out queue-op sections (ENQUEUE/DEQUEUE are internal plumbing)
  const visibleSections = sections.filter((s) => s.kind !== 'queue-op');

  if (visibleSections.length === 0) {
    return (
      <div className="text-[11px]" style={{ color: 'var(--color-text-3)' }}>
        No transcript content
      </div>
    );
  }

  return (
    <div className="space-y-0.5">
      {visibleSections.map((section, si) => (
        <div key={si}>
          {section.kind === 'session-end' ? (
            <div
              className="mt-3 border-t pt-2 text-center text-[10px] italic"
              style={{ borderColor: 'var(--color-border-subtle)', color: 'var(--color-text-4)' }}
            >
              {section.heading}
            </div>
          ) : (
            <>
              <SectionHeader section={section} />
              {section.blocks.map((block, bi) => {
                if (block.type === 'text' && block.content) {
                  return (
                    <div
                      key={bi}
                      className="text-[12px] leading-relaxed py-0.5"
                      style={{ color: 'var(--color-text-1)' }}
                    >
                      <PrMarkdown text={block.content} />
                    </div>
                  );
                }
                if (block.type === 'tool') {
                  return <ToolBlock key={bi} block={block} />;
                }
                if (block.type === 'results') {
                  return (
                    <div
                      key={bi}
                      className="text-[10px] italic py-0.5"
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
      ))}
    </div>
  );
}

// ── Transcript Sidebar (used by TaskDetailView) ──

interface TranscriptSidebarProps {
  session: { entry: SessionEntry; markdown: string | null; loading: boolean };
  onClose: () => void;
  onExpand?: () => void;
}

export function TranscriptSidebar({
  session,
  onClose,
  onExpand,
}: TranscriptSidebarProps): React.ReactElement {
  const s = session.entry;
  const resumeCmd = s.cwd
    ? `cd ${s.cwd} && claude --resume ${s.session_id}`
    : `claude --resume ${s.session_id}`;
  const subtitle = sessionSubtitle(s);

  return (
    <div
      className="flex h-full w-[420px] shrink-0 flex-col border-l"
      style={{ borderColor: 'var(--color-border)', background: 'var(--color-surface-1)' }}
    >
      <div
        className="flex items-center gap-2 border-b px-3 py-2"
        style={{ borderColor: 'var(--color-border)' }}
      >
        <div className="min-w-0 flex-1">
          <div
            className="truncate text-[12px] font-medium"
            style={{ color: 'var(--color-text-1)' }}
          >
            {sessionTitle(s)}
          </div>
          <div className="flex gap-2 text-[10px]" style={{ color: 'var(--color-text-3)' }}>
            {subtitle && <span>{subtitle}</span>}
            {s.model && <span>{s.model}</span>}
            {s.duration_ms != null && s.duration_ms > 0 && (
              <span>
                {s.duration_ms >= 60_000
                  ? `${Math.round(s.duration_ms / 60_000)}m`
                  : `${Math.round(s.duration_ms / 1_000)}s`}
              </span>
            )}
          </div>
        </div>
        <CopyBtn text={resumeCmd} label="-r" />
        {onExpand && (
          <button
            onClick={onExpand}
            title="Expand to full view"
            className="flex items-center justify-center"
            style={{
              width: 20,
              height: 20,
              color: 'var(--color-text-3)',
              background: 'none',
              border: 'none',
              cursor: 'pointer',
            }}
          >
            <svg
              width="14"
              height="14"
              viewBox="0 0 14 14"
              fill="none"
              stroke="currentColor"
              strokeWidth="1.5"
              strokeLinecap="round"
            >
              <path d="M8.5 1.5H12.5V5.5" />
              <path d="M5.5 12.5H1.5V8.5" />
              <path d="M12.5 1.5L8 6" />
              <path d="M1.5 12.5L6 8" />
            </svg>
          </button>
        )}
        <button
          onClick={onClose}
          className="text-sm leading-none"
          style={{
            color: 'var(--color-text-3)',
            background: 'none',
            border: 'none',
            cursor: 'pointer',
          }}
        >
          &times;
        </button>
      </div>
      <div className="flex-1 overflow-auto px-3 py-2">
        {session.loading ? (
          <div className="text-[11px]" style={{ color: 'var(--color-text-3)' }}>
            Loading transcript&hellip;
          </div>
        ) : session.markdown ? (
          <TranscriptViewer markdown={session.markdown} />
        ) : (
          <div className="text-[11px]" style={{ color: 'var(--color-text-3)' }}>
            No transcript available
          </div>
        )}
      </div>
    </div>
  );
}
