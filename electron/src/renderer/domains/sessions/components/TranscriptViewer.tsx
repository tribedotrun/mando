import React, { useMemo } from 'react';
import { Maximize2 } from 'lucide-react';
import type { SessionEntry } from '#renderer/types';
import { CopyBtn } from '#renderer/global/components/CopyBtn';
import { fmtMs } from '#renderer/utils';
import {
  sessionTitle,
  sessionSubtitle,
} from '#renderer/domains/sessions/components/SessionsHelpers';
import {
  SectionRow,
  StructuredOutputBlock,
  type TranscriptSection,
  type ContentBlock,
} from '#renderer/domains/sessions/components/TranscriptBlocks';
import { Button } from '#renderer/components/ui/button';

import { ScrollArea } from '#renderer/components/ui/scroll-area';
import { Skeleton } from '#renderer/components/ui/skeleton';

// Built-in CC tool names -- fallback when format-based check is ambiguous.
// Source: github.com/anthropics/claude-code -> src/tools/ (getAllBaseTools).
// Update when CC adds new tools; stale list degrades gracefully (format check covers most cases).
const CC_TOOLS = new Set([
  'Agent',
  'AskUserQuestion',
  'Bash',
  'Config',
  'CronCreate',
  'CronDelete',
  'CronList',
  'Edit',
  'EnterPlanMode',
  'EnterWorktree',
  'ExitPlanMode',
  'ExitWorktree',
  'Glob',
  'Grep',
  'LSP',
  'ListMcpResourcesTool',
  'NotebookEdit',
  'PowerShell',
  'REPL',
  'Read',
  'ReadMcpResourceTool',
  'RemoteTrigger',
  'SendMessage',
  'SendUserMessage',
  'Skill',
  'Sleep',
  'StructuredOutput',
  'TaskCreate',
  'TaskGet',
  'TaskList',
  'TaskOutput',
  'TaskStop',
  'TaskUpdate',
  'TeamCreate',
  'TeamDelete',
  'TodoWrite',
  'ToolSearch',
  'WebFetch',
  'WebSearch',
  'Write',
]);

function isKnownTool(name: string): boolean {
  return CC_TOOLS.has(name) || name.startsWith('mcp__');
}

// -- Parser --

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
    if (heading.startsWith('Prompt') || heading.startsWith('Human')) kind = 'human';
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
    // Primary: format-based -- Rust renderer emits two spaces after **Name**
    // (with optional modifiers like (bg)), or nothing (code fence on next line).
    // Fallback: known CC tool name or mcp_ prefix.
    const toolMatch = line.match(/^\*\*([\w.-]+)\*\*(.*)$/);
    if (
      toolMatch &&
      (/^( \([^)]+\))* {2}/.test(toolMatch[2]) ||
        (toolMatch[2] === '' && isKnownTool(toolMatch[1])) ||
        (toolMatch[2] === '' && i + 1 < lines.length && lines[i + 1].startsWith('```')))
    ) {
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

    // Initial context -- skip entirely (internal plumbing)
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

// -- Main export --

interface Props {
  markdown: string;
}

export function TranscriptViewer({ markdown }: Props): React.ReactElement {
  const sections = useMemo(() => parseTranscript(markdown), [markdown]);

  // Filter out internal plumbing sections.
  const visibleSections = sections.filter((s) => s.kind !== 'queue-op' && s.kind !== 'session-end');

  if (visibleSections.length === 0) {
    return <div className="text-[11px] text-muted-foreground">No transcript content</div>;
  }

  // Hoist StructuredOutput blocks to the top of the transcript.
  const hoisted = visibleSections.flatMap((s) =>
    s.blocks.filter(
      (b): b is Extract<ContentBlock, { type: 'tool' }> =>
        b.type === 'tool' && b.name === 'StructuredOutput',
    ),
  );

  return (
    <div className="min-w-0 space-y-0.5 [overflow-wrap:anywhere]">
      {hoisted.map((block, i) => (
        <StructuredOutputBlock key={`hoist-${i}`} block={block} />
      ))}
      {visibleSections.map((section, si) => (
        <SectionRow key={si} section={section} />
      ))}
    </div>
  );
}

// -- Transcript Sidebar (used by TaskDetailView) --

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
    <div className="flex h-full w-[440px] shrink-0 flex-col bg-muted">
      {/* Header */}
      <div className="flex items-center gap-2 px-4 py-3">
        <div className="min-w-0 flex-1">
          <div className="truncate text-caption font-medium text-foreground">{sessionTitle(s)}</div>
          <div className="flex gap-2 text-label text-muted-foreground">
            {subtitle && <span>{subtitle}</span>}
            {s.model && <span>{s.model}</span>}
            {s.duration_ms != null && s.duration_ms > 0 && <span>{fmtMs(s.duration_ms)}</span>}
          </div>
        </div>
        <CopyBtn text={resumeCmd} label="-r" />
        {onExpand && (
          <Button variant="ghost" size="icon-xs" onClick={onExpand}>
            <Maximize2 size={14} strokeWidth={1.5} />
          </Button>
        )}
        <Button variant="ghost" size="icon-xs" onClick={onClose}>
          <span className="sr-only">Close</span>
          &#x2715;
        </Button>
      </div>
      {/* Transcript content */}
      <ScrollArea className="min-h-0 flex-1 px-4 py-3">
        {session.loading ? (
          <div className="space-y-3">
            <Skeleton className="h-4 w-3/4" />
            <Skeleton className="h-4 w-1/2" />
            <Skeleton className="h-4 w-full" />
            <Skeleton className="h-4 w-2/3" />
          </div>
        ) : session.markdown ? (
          <TranscriptViewer markdown={session.markdown} />
        ) : (
          <div className="text-caption text-muted-foreground">No transcript available</div>
        )}
      </ScrollArea>
    </div>
  );
}
