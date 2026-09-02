import type {
  AssistantContentBlock,
  AssistantEvent,
  AssistantToolUseBlock,
  TranscriptEvent,
  TaskProvider,
  UserToolResultBlock,
} from '#renderer/global/types';
import { cleanThinkingText } from '#renderer/domains/sessions/service/transcriptRenderHelpers';
import { parseJsonText } from '#result';

/**
 * Map tool_use.id to the `tool_result` block a later user message carried.
 * Used by the renderer to inline a result body on the matching tool call
 * instead of leaking the `user` turn that CC uses as a carrier.
 */
export function indexToolResults(
  events: readonly TranscriptEvent[],
): Map<string, UserToolResultBlock> {
  const out = new Map<string, UserToolResultBlock>();
  for (const event of events) {
    if (event.kind !== 'user') continue;
    for (const block of event.data.blocks) {
      if (block.kind === 'tool_result') {
        out.set(block.data.toolUseId, block.data);
      }
    }
  }
  return out;
}

/**
 * A user event is "carrier-only" if every block is a tool_result — CC uses
 * that shape to ferry results back without an actual human turn. These are
 * rendered inline on the matching tool call instead of as standalone rows.
 */
function isCarrierUserEvent(event: TranscriptEvent): boolean {
  if (event.kind !== 'user') return false;
  if (event.data.blocks.length === 0) return false;
  return event.data.blocks.every((block) => block.kind === 'tool_result');
}

/** Search typed event content rather than opaque React component props. */
export function transcriptEventSearchText(event: TranscriptEvent): string {
  return JSON.stringify(event.data);
}

const SKILL_PROMPT_PREFIX = 'Base directory for this skill:';

/**
 * CC injects every skill body into the transcript as a user text turn that
 * starts with this literal prefix. The renderer collapses those turns so the
 * skill prompt doesn't dominate the transcript.
 */
export function isSkillPromptBody(body: string): boolean {
  return body.startsWith(SKILL_PROMPT_PREFIX);
}

/** Identify prompts injected by Mando's captain rather than written by a human. */
export function mandoPromptLabel(body: string): string | null {
  if (
    /^Read \/.*\/\.ai\/briefs\/[^\n]+ and implement the task described there\./.test(body) &&
    body.includes('/workpad.md')
  ) {
    return 'Task instructions';
  }
  if (
    body.startsWith('You are the task clarifier.') ||
    body.startsWith('You are the clarifier continuing a conversation about a task.')
  ) {
    return 'Clarifier instructions';
  }
  if (body.startsWith("You are the Captain reviewing a worker's output.")) {
    return 'Review instructions';
  }
  if (body.startsWith('You are the Captain merging PR ')) return 'Merge instructions';
  return null;
}

export function isMandoTaskPromptBody(body: string): boolean {
  return mandoPromptLabel(body) !== null;
}

/**
 * Pull a short skill identifier out of the prompt body — the last path
 * segment of the `Base directory for this skill:` line (e.g. `x-land`).
 * Returns `null` when the line is missing or the path is empty.
 */
export function extractSkillName(body: string): string | null {
  const firstLine = body.split('\n', 1)[0]?.trim() ?? '';
  if (!firstLine.startsWith(SKILL_PROMPT_PREFIX)) return null;
  const path = firstLine.slice(SKILL_PROMPT_PREFIX.length).trim();
  if (!path) return null;
  const last = path.split('/').filter(Boolean).pop();
  return last ?? null;
}

interface ToolGroup {
  kind: 'group';
  id: string;
  tools: AssistantToolUseBlock[];
  parentEventIndex: number;
}

type AssistantRenderItem =
  | { kind: 'block'; block: AssistantContentBlock; eventIndex: number; blockIndex: number }
  | { kind: 'group'; group: ToolGroup };

/**
 * Walk an assistant event's content blocks and collapse every consecutive
 * tool run into a `ToolGroup`. Thought and text blocks remain the visual
 * boundaries that explain why the actions happened.
 */
export function groupAssistantBlocks(
  event: AssistantEvent,
  eventIndex: number,
): AssistantRenderItem[] {
  const out: AssistantRenderItem[] = [];
  let pendingGroup: AssistantToolUseBlock[] = [];
  let groupStart = 0;

  const flushGroup = () => {
    if (pendingGroup.length === 0) return;
    if (pendingGroup.length === 1) {
      out.push({
        kind: 'block',
        block: { kind: 'tool_use', data: pendingGroup[0]! },
        eventIndex,
        blockIndex: groupStart,
      });
    } else {
      out.push({
        kind: 'group',
        group: {
          kind: 'group',
          id: `activity-${pendingGroup[0]!.id}`,
          tools: pendingGroup,
          parentEventIndex: eventIndex,
        },
      });
    }
    pendingGroup = [];
  };

  event.blocks.forEach((block, blockIndex) => {
    if (block.kind === 'tool_use') {
      if (pendingGroup.length === 0) {
        groupStart = blockIndex;
      }
      pendingGroup.push(block.data);
      return;
    }
    if (
      (block.kind === 'text' && !block.data.text.trim()) ||
      ((block.kind === 'thinking' || block.kind === 'advisor_tool_result') &&
        !cleanThinkingText(block.data.text))
    ) {
      return;
    }
    flushGroup();
    out.push({ kind: 'block', block, eventIndex, blockIndex });
  });

  flushGroup();
  return out;
}

export type TranscriptRenderRow =
  | {
      kind: 'event';
      id: string;
      event: TranscriptEvent;
      eventIndex: number;
      searchEvents: TranscriptEvent[];
    }
  | {
      kind: 'tool_group';
      id: string;
      group: ToolGroup;
      searchEvents: TranscriptEvent[];
    };

/**
 * Group tool-only assistant events across the protocol carrier rows Codex
 * inserts between them. A thought, assistant text, human turn, or visible
 * system event closes the group so actions stay attached to their intent.
 */
export function buildTranscriptRenderRows(
  events: readonly TranscriptEvent[],
): TranscriptRenderRow[] {
  const rows: TranscriptRenderRow[] = [];
  let pendingEvents: Array<{ event: TranscriptEvent; eventIndex: number }> = [];
  let pendingSearchEvents: TranscriptEvent[] = [];
  let pendingTools: AssistantToolUseBlock[] = [];

  const flushTools = () => {
    if (pendingTools.length === 0) return;
    const first = pendingEvents[0]!;
    if (pendingTools.length === 1) {
      rows.push({
        kind: 'event',
        id: `event-${first.eventIndex}`,
        event: first.event,
        eventIndex: first.eventIndex,
        searchEvents: pendingSearchEvents,
      });
    } else {
      const id = `activity-${pendingTools[0]!.id}`;
      rows.push({
        kind: 'tool_group',
        id,
        group: {
          kind: 'group',
          id,
          tools: pendingTools,
          parentEventIndex: first.eventIndex,
        },
        searchEvents: pendingSearchEvents,
      });
    }
    pendingEvents = [];
    pendingSearchEvents = [];
    pendingTools = [];
  };

  events.forEach((event, eventIndex) => {
    const tools = toolOnlyAssistantBlocks(event);
    if (tools) {
      pendingEvents.push({ event, eventIndex });
      pendingSearchEvents.push(event);
      pendingTools.push(...tools);
      return;
    }
    if (isTransparentActivityEvent(event)) {
      if (pendingTools.length > 0 && isCarrierUserEvent(event)) {
        pendingSearchEvents.push(event);
      }
      return;
    }
    flushTools();
    rows.push({
      kind: 'event',
      id: `event-${eventIndex}`,
      event,
      eventIndex,
      searchEvents: [event],
    });
  });

  flushTools();
  return rows;
}

/**
 * Claude and Codex streams have different carrier protocols. Codex emits
 * separate activity items that can be grouped across carrier rows; Claude
 * emits complete API messages split into block envelopes sharing messageId.
 */
export function buildProviderTranscriptRenderRows(
  events: readonly TranscriptEvent[],
  provider: TaskProvider | undefined,
): TranscriptRenderRow[] {
  if (provider !== 'claude') return buildTranscriptRenderRows(events);
  return events.flatMap((event, eventIndex) => {
    if (isTransparentActivityEvent(event) || isClaudeImageMetadataEvent(event)) return [];
    return [
      {
        kind: 'event' as const,
        id: `event-${eventIndex}`,
        event,
        eventIndex,
        searchEvents: [event],
      },
    ];
  });
}

const CLAUDE_IMAGE_METADATA =
  /^\[Image: original \d+x\d+, displayed at \d+x\d+\. Multiply coordinates by \d+(?:\.\d+)? to map to original image\.\]$/;

/** Claude Code emits this internal sizing hint immediately after an image tool result. */
function isClaudeImageMetadataEvent(event: TranscriptEvent): boolean {
  if (event.kind !== 'user' || event.data.blocks.length === 0) return false;
  return event.data.blocks.every(
    (block) => block.kind === 'text' && CLAUDE_IMAGE_METADATA.test(block.data.text.trim()),
  );
}

function toolOnlyAssistantBlocks(event: TranscriptEvent): AssistantToolUseBlock[] | null {
  if (event.kind !== 'assistant' || event.data.blocks.length === 0) return null;
  const tools: AssistantToolUseBlock[] = [];
  for (const block of event.data.blocks) {
    if (block.kind !== 'tool_use') return null;
    tools.push(block.data);
  }
  return tools;
}

function isTransparentActivityEvent(event: TranscriptEvent): boolean {
  return (
    isCarrierUserEvent(event) ||
    event.kind === 'system_hook' ||
    event.kind === 'system_thinking_tokens' ||
    event.kind === 'system_claude_progress' ||
    event.kind === 'system_token_usage' ||
    event.kind === 'tool_progress'
  );
}

/**
 * Return events along the primary conversation branch — filters out events
 * flagged `isSidechain: true` so a sub-agent transcript doesn't drown the
 * main thread. Sidechain expansion is a future UI affordance.
 */
export function resolveActiveBranch(
  events: readonly TranscriptEvent[],
  provider?: TaskProvider,
): TranscriptEvent[] {
  const branch = events.filter((event) => {
    const isSide = metaOf(event)?.isSidechain === true;
    return !isSide;
  });
  return provider === 'claude'
    ? coalesceClaudeAssistantMessages(branch)
    : coalesceStreamingAssistantDeltas(branch);
}

function metaOf(event: TranscriptEvent) {
  switch (event.kind) {
    case 'system_init':
    case 'system_compact_boundary':
    case 'system_status':
    case 'system_api_retry':
    case 'system_local_command_output':
    case 'system_hook':
    case 'system_rate_limit':
    case 'system_thinking_tokens':
    case 'system_claude_progress':
    case 'system_token_usage':
    case 'user':
    case 'assistant':
    case 'tool_progress':
    case 'result':
    case 'unknown':
      return event.data.meta;
  }
}

function coalesceClaudeAssistantMessages(events: readonly TranscriptEvent[]): TranscriptEvent[] {
  const out: TranscriptEvent[] = [];
  const messageIndexes = new Map<string, number>();
  for (const event of events) {
    if (event.kind !== 'assistant' || !event.data.messageId) {
      out.push(event);
      continue;
    }
    const priorIndex = messageIndexes.get(event.data.messageId);
    if (priorIndex === undefined) {
      messageIndexes.set(event.data.messageId, out.length);
      out.push(event);
      continue;
    }
    const prior = out[priorIndex];
    if (!prior || prior.kind !== 'assistant') {
      out.push(event);
      continue;
    }
    out[priorIndex] = {
      ...prior,
      data: {
        ...prior.data,
        model: event.data.model ?? prior.data.model,
        usage: event.data.usage ?? prior.data.usage,
        stopReason: event.data.stopReason ?? prior.data.stopReason,
        blocks: [...prior.data.blocks, ...event.data.blocks],
      },
    };
  }
  return out;
}

export interface ClaudeRateLimitWindow {
  name: string;
  utilization: number | null;
  resetsAt: number | null;
}

export interface ClaudeRateLimitInfo {
  status: string;
  rateLimitType: string | null;
  resetsAt: number | null;
  overageStatus: string | null;
  overageDisabledReason: string | null;
  isUsingOverage: boolean | null;
  windows: ClaudeRateLimitWindow[];
}

export function parseClaudeRateLimitInfo(raw: string): ClaudeRateLimitInfo | null {
  const parsed = parseJsonText(raw, 'transcriptEvents:claudeRateLimit');
  if (parsed.isErr()) return null;
  const root = recordValue(parsed.value);
  if (!root) return null;
  const unifiedWindows = recordValue(root.unifiedWindows);
  const windows = Object.entries(unifiedWindows ?? {}).map(([name, value]) => {
    const window = recordValue(value);
    return {
      name,
      utilization: finiteNumber(window?.utilization),
      resetsAt: finiteNumber(window?.resetsAt),
    };
  });
  return {
    status: stringValue(root.status) ?? 'unknown',
    rateLimitType: stringValue(root.rateLimitType),
    resetsAt: finiteNumber(root.resetsAt),
    overageStatus: stringValue(root.overageStatus),
    overageDisabledReason: stringValue(root.overageDisabledReason),
    isUsingOverage: typeof root.isUsingOverage === 'boolean' ? root.isUsingOverage : null,
    windows,
  };
}

function recordValue(value: unknown): Record<string, unknown> | null {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) return null;
  return value as Record<string, unknown>;
}

function finiteNumber(value: unknown): number | null {
  return typeof value === 'number' && Number.isFinite(value) ? value : null;
}

function stringValue(value: unknown): string | null {
  return typeof value === 'string' ? value : null;
}

function coalesceStreamingAssistantDeltas(events: readonly TranscriptEvent[]): TranscriptEvent[] {
  const out: TranscriptEvent[] = [];
  for (const event of events) {
    const previous = out[out.length - 1];
    if (previous && canMergeAssistantDelta(previous, event)) {
      out[out.length - 1] = mergeAssistantDelta(previous, event);
      continue;
    }
    out.push(event);
  }
  return out;
}

function canMergeAssistantDelta(left: TranscriptEvent, right: TranscriptEvent): boolean {
  if (left.kind !== 'assistant' || right.kind !== 'assistant') return false;
  const leftUuid = left.data.meta.uuid;
  if (!leftUuid || leftUuid !== right.data.meta.uuid) return false;
  const leftBlock = left.data.blocks[0];
  const rightBlock = right.data.blocks[0];
  if (
    !leftBlock ||
    !rightBlock ||
    left.data.blocks.length !== 1 ||
    right.data.blocks.length !== 1
  ) {
    return false;
  }
  return (
    (leftBlock.kind === 'text' && rightBlock.kind === 'text') ||
    (leftBlock.kind === 'thinking' && rightBlock.kind === 'thinking')
  );
}

function mergeAssistantDelta(left: TranscriptEvent, right: TranscriptEvent): TranscriptEvent {
  if (left.kind !== 'assistant' || right.kind !== 'assistant') return left;
  const leftBlock = left.data.blocks[0];
  const rightBlock = right.data.blocks[0];
  if (!leftBlock || !rightBlock) return left;
  if (leftBlock.kind === 'text' && rightBlock.kind === 'text') {
    return {
      ...left,
      data: {
        ...left.data,
        usage: right.data.usage ?? left.data.usage,
        stopReason: right.data.stopReason ?? left.data.stopReason,
        blocks: [{ kind: 'text', data: { text: leftBlock.data.text + rightBlock.data.text } }],
      },
    };
  }
  if (leftBlock.kind === 'thinking' && rightBlock.kind === 'thinking') {
    return {
      ...left,
      data: {
        ...left.data,
        usage: right.data.usage ?? left.data.usage,
        stopReason: right.data.stopReason ?? left.data.stopReason,
        blocks: [{ kind: 'thinking', data: { text: leftBlock.data.text + rightBlock.data.text } }],
      },
    };
  }
  return left;
}

export function unknownEventTitle(event: { rawType: string | null; rawSubtype: string | null }) {
  const base = event.rawType ?? 'app-server event';
  return event.rawSubtype ? `${base} · ${event.rawSubtype}` : base;
}
