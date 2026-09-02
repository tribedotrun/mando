import type { TaskProvider, TranscriptEvent, TranscriptUsageInfo } from '#renderer/global/types';
import { parseJsonText } from '#result';

interface TranscriptTokenUsageSummary extends TranscriptUsageInfo {
  totalTokens: number;
  source: 'result' | 'result_sum' | 'system_token_usage' | 'token_usage_event' | 'assistant_sum';
}

export function summarizeTranscriptTokenUsage(
  events: readonly TranscriptEvent[],
  provider?: TaskProvider,
): TranscriptTokenUsageSummary | null {
  let resultUsage: TranscriptUsageInfo | null = null;
  let resultSum: TranscriptUsageInfo | null = null;
  let systemTokenUsage: TranscriptUsageInfo | null = null;
  let tokenUsageEvent: TranscriptTokenUsageSummary | null = null;
  let assistantSum: TranscriptUsageInfo | null = null;
  const claudeMessageIds = new Set<string>();

  for (const event of events) {
    if (event.kind === 'result' && event.data.summary.usage) {
      resultUsage = event.data.summary.usage;
      resultSum = addUsage(resultSum, event.data.summary.usage);
      continue;
    }
    if (event.kind === 'system_token_usage') {
      systemTokenUsage = event.data.usage;
      continue;
    }
    if (event.kind === 'unknown') {
      tokenUsageEvent = tokenUsageEventFromRaw(event.data.raw) ?? tokenUsageEvent;
      continue;
    }
    if (event.kind === 'assistant' && event.data.usage) {
      if (provider === 'claude' && event.data.messageId) {
        if (claudeMessageIds.has(event.data.messageId)) continue;
        claudeMessageIds.add(event.data.messageId);
      }
      assistantSum = addUsage(assistantSum, event.data.usage);
    }
  }

  if (provider === 'claude') {
    return withTotal(resultSum, 'result_sum') ?? withTotal(assistantSum, 'assistant_sum');
  }
  return (
    withTotal(systemTokenUsage, 'system_token_usage') ??
    tokenUsageEvent ??
    withTotal(resultUsage, 'result') ??
    withTotal(assistantSum, 'assistant_sum')
  );
}

export function formatExactTokenCount(tokens: number): string {
  return new Intl.NumberFormat('en-US').format(tokens);
}

export function formatUsageBreakdown(usage: TranscriptUsageInfo): string {
  return [
    `input ${formatExactTokenCount(usage.input_tokens)}`,
    `output ${formatExactTokenCount(usage.output_tokens)}`,
    `cache read ${formatExactTokenCount(usage.cache_read_tokens)}`,
    `cache write ${formatExactTokenCount(usage.cache_creation_tokens)}`,
  ].join(' · ');
}

function totalUsageTokens(usage: TranscriptUsageInfo): number {
  return (
    usage.input_tokens + usage.output_tokens + usage.cache_read_tokens + usage.cache_creation_tokens
  );
}

function withTotal(
  usage: TranscriptUsageInfo | null,
  source: TranscriptTokenUsageSummary['source'],
): TranscriptTokenUsageSummary | null {
  if (!usage) return null;
  const totalTokens = totalUsageTokens(usage);
  if (totalTokens <= 0) return null;
  return { ...usage, totalTokens, source };
}

function addUsage(
  left: TranscriptUsageInfo | null,
  right: TranscriptUsageInfo,
): TranscriptUsageInfo {
  if (!left) return { ...right };
  return {
    input_tokens: left.input_tokens + right.input_tokens,
    output_tokens: left.output_tokens + right.output_tokens,
    cache_read_tokens: left.cache_read_tokens + right.cache_read_tokens,
    cache_creation_tokens: left.cache_creation_tokens + right.cache_creation_tokens,
  };
}

function tokenUsageEventFromRaw(raw: string): TranscriptTokenUsageSummary | null {
  const parsed = parseJsonText(raw, 'transcriptEvents:tokenUsage');
  if (parsed.isErr()) return null;
  const root = asRecord(parsed.value);
  if (!root) return null;
  const usage =
    recordAt(root, ['params', 'tokenUsage']) ??
    recordAt(root, ['params', 'turn', 'tokenUsage']) ??
    recordAt(root, ['params', 'usage']) ??
    recordAt(root, ['result', 'turn', 'tokenUsage']) ??
    recordAt(root, ['result', 'tokenUsage']);
  return summaryFromUsageRecord(usage);
}

function summaryFromUsageRecord(
  rawUsage: Record<string, unknown> | null,
): TranscriptTokenUsageSummary | null {
  if (!rawUsage) return null;
  const usage = asRecord(rawUsage.total) ?? rawUsage;
  const info = {
    input_tokens: tokenCount(usage, ['inputTokens', 'input_tokens']),
    output_tokens: tokenCount(usage, ['outputTokens', 'output_tokens']),
    cache_read_tokens: tokenCount(usage, ['cachedInputTokens', 'cache_read_input_tokens']),
    cache_creation_tokens: tokenCount(usage, [
      'cacheCreationInputTokens',
      'cache_creation_input_tokens',
    ]),
  };
  info.input_tokens = Math.max(
    0,
    info.input_tokens - info.cache_read_tokens - info.cache_creation_tokens,
  );
  const totalTokens = tokenCount(usage, ['totalTokens', 'total_tokens']) || totalUsageTokens(info);
  if (totalTokens <= 0) return null;
  return { ...info, totalTokens, source: 'token_usage_event' };
}

function recordAt(
  root: Record<string, unknown>,
  path: readonly string[],
): Record<string, unknown> | null {
  let current: Record<string, unknown> | null = root;
  for (const segment of path) {
    current = asRecord(current?.[segment]);
    if (!current) return null;
  }
  return current;
}

function asRecord(value: unknown): Record<string, unknown> | null {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) return null;
  return value as Record<string, unknown>;
}

function tokenCount(value: Record<string, unknown>, names: readonly string[]): number {
  for (const name of names) {
    const raw = value[name];
    if (typeof raw === 'number' && Number.isFinite(raw) && raw > 0) {
      return Math.floor(raw);
    }
  }
  return 0;
}
