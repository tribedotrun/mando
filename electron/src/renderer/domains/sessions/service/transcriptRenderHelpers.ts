import type {
  AssistantToolUseBlock,
  GrepInput,
  ReadInput,
  ResultOutcome,
  UserToolResultBlock,
} from '#renderer/global/types';
import { toolLabel } from '#renderer/domains/sessions/service/transcriptEvents';
import { parseJsonText } from '#result';

export function humanOutcome(outcome: ResultOutcome): string {
  switch (outcome) {
    case 'success':
      return 'success';
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

export function toolGroupSummary(tools: AssistantToolUseBlock[]): string {
  const counts = new Map<string, number>();
  for (const t of tools) {
    const label = toolLabel(t.name);
    counts.set(label, (counts.get(label) ?? 0) + 1);
  }
  return [...counts.entries()].map(([k, v]) => (v > 1 ? `${v} ${k}` : k)).join(' · ');
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
      if (b.kind === 'image') return '[image]';
      return b.data.raw;
    })
    .join('\n');
}

export function prettyJson(raw: string): string {
  const parsed = parseJsonText(raw, 'transcriptEvents:prettyJson');
  if (parsed.isErr()) return raw;
  return JSON.stringify(parsed.value, null, 2);
}
