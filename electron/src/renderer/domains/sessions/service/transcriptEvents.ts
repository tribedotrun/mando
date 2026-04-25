import type {
  AssistantContentBlock,
  AssistantEvent,
  AssistantToolUseBlock,
  TranscriptEvent,
  ToolName,
  UserToolResultBlock,
} from '#renderer/global/types';

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
export function isCarrierUserEvent(event: TranscriptEvent): boolean {
  if (event.kind !== 'user') return false;
  if (event.data.blocks.length === 0) return false;
  return event.data.blocks.every((block) => block.kind === 'tool_result');
}

/**
 * Tools whose usage is read-only/search and collapses cleanly when grouped
 * with its neighbors. Mirrors CC's `getToolSearchOrReadInfo` — Write / Edit /
 * Bash are intentionally excluded (they mutate state and need explicit UI).
 */
export function isCollapsibleTool(name: ToolName): boolean {
  switch (name.kind) {
    case 'read':
    case 'grep':
    case 'glob':
      return true;
    default:
      return false;
  }
}

export interface ToolGroup {
  kind: 'group';
  id: string;
  tools: AssistantToolUseBlock[];
  parentEventIndex: number;
}

export type AssistantRenderItem =
  | { kind: 'block'; block: AssistantContentBlock; eventIndex: number; blockIndex: number }
  | { kind: 'group'; group: ToolGroup };

/**
 * Walk an assistant event's content blocks and collapse consecutive
 * collapsible tool uses into `ToolGroup` entries. Non-collapsible blocks
 * (text, thinking, write/edit/bash tool uses, unknowns) surface one-per-row.
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
          id: `${eventIndex}-${groupStart}`,
          tools: pendingGroup,
          parentEventIndex: eventIndex,
        },
      });
    }
    pendingGroup = [];
  };

  event.blocks.forEach((block, blockIndex) => {
    if (block.kind === 'tool_use' && isCollapsibleTool(block.data.name)) {
      if (pendingGroup.length === 0) {
        groupStart = blockIndex;
      }
      pendingGroup.push(block.data);
      return;
    }
    flushGroup();
    out.push({ kind: 'block', block, eventIndex, blockIndex });
  });

  flushGroup();
  return out;
}

/**
 * Return events along the primary conversation branch — filters out events
 * flagged `isSidechain: true` so a sub-agent transcript doesn't drown the
 * main thread. Sidechain expansion is a future UI affordance.
 */
export function resolveActiveBranch(events: readonly TranscriptEvent[]): TranscriptEvent[] {
  return events.filter((event) => {
    const isSide = metaOf(event)?.isSidechain === true;
    return !isSide;
  });
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
    case 'user':
    case 'assistant':
    case 'tool_progress':
    case 'result':
    case 'unknown':
      return event.data.meta;
  }
}

/**
 * Friendly label for a `ToolName` — used by every per-tool block header so
 * the casing stays consistent between MCP variants and built-ins.
 */
export function toolLabel(name: ToolName): string {
  switch (name.kind) {
    case 'bash':
      return 'Bash';
    case 'read':
      return 'Read';
    case 'edit':
      return 'Edit';
    case 'write':
      return 'Write';
    case 'grep':
      return 'Grep';
    case 'glob':
      return 'Glob';
    case 'todo_write':
      return 'Todo';
    case 'web_fetch':
      return 'WebFetch';
    case 'web_search':
      return 'WebSearch';
    case 'task':
      return 'Task';
    case 'notebook_edit':
      return 'NotebookEdit';
    case 'skill':
      return 'Skill';
    case 'structured_output':
      return 'StructuredOutput';
    case 'mcp':
      return `${name.data.server}/${name.data.tool}`;
    case 'other':
      return name.data.name;
  }
}
