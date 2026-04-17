// -- Types (shared with TranscriptViewer and TranscriptBlocks) --

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

// -- Pure helpers --

export function cleanLabel(label: string): string {
  return label.replace(/\*([^*]+)\*/g, '$1').replace(/`([^`]+)`/g, '$1');
}

export function looksLikeTerminalOutput(text: string): boolean {
  // eslint-disable-next-line no-control-regex
  return /\x1b\[/.test(text) || /^\$\s/.test(text);
}

export function toolLangToShiki(name: string, lang?: string): string {
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

export function parseTranscript(markdown: string): TranscriptSection[] {
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

/** Filter out internal queue-op sections that are not user-visible. */
export function filterVisibleSections(sections: TranscriptSection[]): TranscriptSection[] {
  return sections.filter((s) => s.kind !== 'queue-op');
}

/** Extract all StructuredOutput tool blocks from sections (for hoisting above the transcript). */
export function hoistStructuredOutputs(
  sections: TranscriptSection[],
): Extract<ContentBlock, { type: 'tool' }>[] {
  return sections.flatMap((s) =>
    s.blocks.filter(
      (b): b is Extract<ContentBlock, { type: 'tool' }> =>
        b.type === 'tool' && b.name === 'StructuredOutput',
    ),
  );
}

/** Extract text blocks with content from a section. */
export function textBlocksOf(
  section: TranscriptSection,
): Extract<ContentBlock, { type: 'text' }>[] {
  return section.blocks.filter(
    (b): b is Extract<ContentBlock, { type: 'text' }> => b.type === 'text' && !!b.content,
  );
}

/** Extract tool/results/error blocks (excluding StructuredOutput) from a section. */
export function toolBlocksOf(section: TranscriptSection): ContentBlock[] {
  return section.blocks.filter(
    (b) =>
      (b.type === 'tool' && b.name !== 'StructuredOutput') ||
      b.type === 'results' ||
      b.type === 'error',
  );
}

/** Count actual tool-call blocks across multiple sections. */
export function countToolCalls(sections: TranscriptSection[]): number {
  return sections.reduce(
    (sum, t) =>
      sum + t.blocks.filter((b) => b.type === 'tool' && b.name !== 'StructuredOutput').length,
    0,
  );
}

// -- Grouping --

export function groupSections(sections: TranscriptSection[]): PromptGroup[] {
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
