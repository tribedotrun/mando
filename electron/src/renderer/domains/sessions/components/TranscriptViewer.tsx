import React, { useMemo } from 'react';
import {
  PromptGroupRow,
  StructuredOutputBlock,
  type TranscriptSection,
  type ContentBlock,
  type PromptGroup,
} from '#renderer/domains/sessions/components/TranscriptBlocks';

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

  const raw = markdown.split(/\n---\n## /);
  const sections: TranscriptSection[] = [];

  for (let i = 0; i < raw.length; i++) {
    let chunk = raw[i];
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
        i++;
      } else {
        i++;
      }
      blocks.push({ type: 'tool', name, label, body: codeBody, lang });
      continue;
    }

    if (line.startsWith('*results:') || line.startsWith('*results ')) {
      flushText();
      blocks.push({ type: 'results', content: line.replace(/^\*|\*$/g, '') });
      i++;
      continue;
    }

    if (line.startsWith('**Error:**')) {
      flushText();
      let codeBody = '';
      if (i + 1 < lines.length && lines[i + 1].startsWith('```')) {
        i += 2;
        const codeLines: string[] = [];
        while (i < lines.length && !lines[i].startsWith('```')) {
          codeLines.push(lines[i]);
          i++;
        }
        codeBody = codeLines.join('\n');
        i++;
      } else {
        i++;
      }
      blocks.push({ type: 'error', body: codeBody });
      continue;
    }

    if (line.startsWith('**Initial context:**')) {
      if (i + 1 < lines.length && lines[i + 1].startsWith('```')) {
        i += 2;
        while (i < lines.length && !lines[i].startsWith('```')) i++;
        i++;
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

// -- Grouping --

function groupSections(sections: TranscriptSection[]): PromptGroup[] {
  const groups: PromptGroup[] = [];
  let current: PromptGroup = { prompt: null, turns: [] };

  for (const section of sections) {
    if (section.kind === 'human' || section.kind === 'session-end') {
      if (current.prompt || current.turns.length > 0) {
        groups.push(current);
      }
      current = { prompt: section, turns: [] };
    } else if (section.kind === 'turn') {
      current.turns.push(section);
    }
  }

  if (current.prompt || current.turns.length > 0) {
    groups.push(current);
  }

  return groups;
}

// -- Main export --

interface Props {
  markdown: string;
}

export function TranscriptViewer({ markdown }: Props): React.ReactElement {
  const sections = useMemo(() => parseTranscript(markdown), [markdown]);

  const visibleSections = sections.filter((s) => s.kind !== 'queue-op');

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

  const groups = groupSections(visibleSections);

  return (
    <div className="min-w-0 divide-y divide-border [overflow-wrap:anywhere]">
      {hoisted.map((block, i) => (
        <StructuredOutputBlock key={`hoist-${i}`} block={block} />
      ))}
      {groups.map((group, gi) => (
        <PromptGroupRow key={gi} group={group} />
      ))}
    </div>
  );
}
