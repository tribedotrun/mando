import type {
  AssistantToolUseBlock,
  FileChangeEntry,
  FileChangeInput,
  GrepInput,
  ReadInput,
  ResultOutcome,
  TranscriptUsageInfo,
  UserToolResultBlock,
} from '#renderer/global/types';
import { parseJsonText } from '#result';

export function humanOutcome(outcome: ResultOutcome): string {
  switch (outcome) {
    case 'success':
      return 'success';
    case 'interrupted':
      return 'interrupted';
    case 'error_max_turns':
      return 'max turns';
    case 'error_max_budget_usd':
      return 'max budget';
    case 'error_max_structured_output_retries':
      return 'max structured-output retries';
    case 'error_during_execution':
      return 'error during execution';
  }
}

export function formatDuration(ms: number): string {
  if (ms < 1000) return `${ms}ms`;
  const sec = ms / 1000;
  if (sec < 60) return `${sec.toFixed(1)}s`;
  const min = sec / 60;
  return `${min.toFixed(1)}m`;
}

export function formatCost(usd: number): string {
  return `$${usd.toFixed(4)}`;
}

export function formatTotalUsage(usage: TranscriptUsageInfo | null): string | null {
  if (!usage) return null;
  const tokens =
    usage.input_tokens +
    usage.output_tokens +
    usage.cache_read_tokens +
    usage.cache_creation_tokens;
  if (tokens <= 0) return null;
  return `${tokens} tok`;
}

export function buildGrepSummary(input: GrepInput): string {
  const parts: string[] = [input.pattern];
  if (input.path) parts.push(`in ${input.path}`);
  if (input.glob) parts.push(`glob ${input.glob}`);
  if (input.fileType) parts.push(`type ${input.fileType}`);
  return parts.join(' · ');
}

export function buildReadSummary(input: ReadInput): string {
  const parts: string[] = [input.filePath];
  if (input.offset != null && input.limit != null) {
    parts.push(`lines ${input.offset}..${input.offset + input.limit}`);
  } else if (input.limit != null) {
    parts.push(`first ${input.limit} lines`);
  } else if (input.offset != null) {
    parts.push(`from line ${input.offset}`);
  }
  if (input.pages) parts.push(`pages ${input.pages}`);
  return parts.join(' · ');
}

export function cleanThinkingText(text: string): string {
  const trimmed = text.trim();
  const withoutStrong = trimmed.replace(/^\*\*(.*?)\*\*$/s, '$1').trim();
  const segments = withoutStrong
    .split(/\*{4}/)
    .map((segment) => segment.replace(/\*\*/g, '').trim())
    .filter(Boolean);
  if (segments.length > 1 && segments.every((segment) => segment === segments[0])) {
    return segments[0]!;
  }
  const normalized = segments.join(' · ');
  const midpoint = Math.floor(normalized.length / 2);
  if (normalized.length % 2 === 0 && normalized.slice(0, midpoint) === normalized.slice(midpoint)) {
    return normalized.slice(0, midpoint).trim();
  }
  return normalized;
}

export function bashSummary(command: string): string {
  const trimmed = command.trim();
  const match = trimmed.match(/^\/(?:bin|usr\/bin)\/(?:zsh|bash|sh)\s+-lc\s+(["'])([\s\S]*)\1$/);
  return match?.[2]?.trim() || trimmed;
}

export function fileChangeSummary(input: FileChangeInput): string {
  if (input.changes.length === 0) return 'No files changed';
  if (input.changes.length === 1) {
    const change = input.changes[0]!;
    return `${fileChangeVerb(change)} ${compactDisplayPath(change.path)}`;
  }
  return `Changed ${input.changes.length} files`;
}

export function compactDisplayPath(path: string): string {
  if (!path.startsWith('/')) return path;
  const parts = path.split('/').filter(Boolean);
  return parts.length > 3 ? `…/${parts.slice(-3).join('/')}` : path;
}

export function fileChangeVerb(change: FileChangeEntry): string {
  switch (change.kind) {
    case 'add':
      return 'Created';
    case 'delete':
      return 'Deleted';
    case 'move':
      return 'Moved';
    case 'update':
    case 'other':
      return 'Edited';
  }
}

export function diffLineTone(line: string): string {
  if (line.startsWith('+') && !line.startsWith('+++')) return 'text-success';
  if (line.startsWith('-') && !line.startsWith('---')) return 'text-destructive';
  if (line.startsWith('@@')) return 'text-primary';
  return 'text-muted-foreground';
}

export function toolGroupSummary(tools: AssistantToolUseBlock[]): string {
  let commands = 0;
  let reads = 0;
  let searches = 0;
  let changedFiles = 0;
  let viewedImages = 0;
  let delegatedTasks = 0;
  let otherTools = 0;

  for (const tool of tools) {
    switch (tool.name.kind) {
      case 'bash':
        commands++;
        break;
      case 'read':
        reads++;
        break;
      case 'grep':
      case 'glob':
      case 'web_fetch':
      case 'web_search':
        searches++;
        break;
      case 'edit':
      case 'write':
      case 'notebook_edit':
        changedFiles++;
        break;
      case 'file_change':
        changedFiles +=
          tool.input.kind === 'file_change' ? Math.max(tool.input.data.changes.length, 1) : 1;
        break;
      case 'image_view':
        viewedImages++;
        break;
      case 'task':
        delegatedTasks++;
        break;
      case 'todo_write':
      case 'skill':
      case 'structured_output':
      case 'mcp':
      case 'other':
        otherTools++;
        break;
    }
  }

  const parts: string[] = [];
  if (commands > 0) parts.push(`Ran ${countNoun(commands, 'command')}`);
  if (reads > 0) parts.push(`Read ${countNoun(reads, 'file')}`);
  if (searches > 0) parts.push(`Searched ${countNoun(searches, 'source')}`);
  if (changedFiles > 0) parts.push(`Changed ${countNoun(changedFiles, 'file')}`);
  if (viewedImages > 0) parts.push(`Viewed ${countNoun(viewedImages, 'image')}`);
  if (delegatedTasks > 0) parts.push(`Delegated ${countNoun(delegatedTasks, 'task')}`);
  if (otherTools > 0) parts.push(`Used ${countNoun(otherTools, 'tool')}`);
  return parts.join(' · ');
}

function countNoun(count: number, noun: string): string {
  return `${count} ${noun}${count === 1 ? '' : 's'}`;
}

export function todoMarker(status: 'pending' | 'in_progress' | 'completed'): string {
  if (status === 'completed') return '[x]';
  if (status === 'in_progress') return '[~]';
  return '[ ]';
}

export function extractToolResultText(result: UserToolResultBlock): string {
  if (result.content.kind === 'text') return result.content.data.text;
  return result.content.data.blocks
    .map((b) => {
      if (b.kind === 'text') return b.data.text;
      if (b.kind === 'image') return '';
      return b.data.raw;
    })
    .filter(Boolean)
    .join('\n');
}

export function prettyJson(raw: string): string {
  const parsed = parseJsonText(raw, 'transcriptEvents:prettyJson');
  if (parsed.isErr()) return raw;
  return JSON.stringify(parsed.value, null, 2);
}
