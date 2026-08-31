import assert from 'node:assert/strict';
import { describe, it } from 'node:test';

import { buildResumeCmd } from '../resumeCommand.ts';
import {
  bashSummary,
  cleanThinkingText,
  compactDisplayPath,
  diffLineTone,
  fileChangeSummary,
  toolGroupSummary,
} from '../transcriptRenderHelpers.ts';
import {
  buildTranscriptRenderRows,
  groupAssistantBlocks,
  isMandoTaskPromptBody,
  mandoPromptLabel,
  transcriptEventSearchText,
} from '../transcriptEvents.ts';
import type {
  AssistantToolUseBlock,
  EventMeta,
  FileChangeInput,
  TranscriptEvent,
} from '#renderer/global/types';

function eventMeta(lineNumber: number, uuid: string): EventMeta {
  return {
    index: { lineNumber },
    uuid,
    parentUuid: null,
    sessionId: 'session-1',
    timestamp: null,
    isSidechain: null,
  };
}

function bashTool(id: string, command: string): AssistantToolUseBlock {
  return {
    id,
    name: { kind: 'bash' },
    input: {
      kind: 'bash',
      data: {
        command,
        description: null,
        timeout: null,
        runInBackground: null,
      },
    },
  };
}

function assistantEvent(
  lineNumber: number,
  block: import('#renderer/global/types').AssistantContentBlock,
): TranscriptEvent {
  return {
    kind: 'assistant',
    data: {
      meta: eventMeta(lineNumber, `event-${lineNumber}`),
      model: 'gpt-5.6-luna',
      blocks: [block],
      usage: null,
      stopReason: null,
    },
  };
}

describe('buildResumeCmd', () => {
  it('uses the OpenCode TUI resume command without requiring a run prompt', () => {
    assert.equal(buildResumeCmd('ses_open', 'opencode'), 'opencode --session ses_open');
    assert.equal(
      buildResumeCmd('ses_open', 'opencode', '/tmp/worktree'),
      'cd "/tmp/worktree" && opencode --session ses_open',
    );
  });

  it('makes content inside typed component events searchable', () => {
    const event = {
      kind: 'assistant',
      data: {
        meta: {
          index: { lineNumber: 1 },
          uuid: 'reasoning-1',
          parentUuid: null,
          sessionId: 'session-1',
          timestamp: null,
          isSidechain: null,
        },
        model: null,
        blocks: [{ kind: 'thinking', data: { text: 'Inspecting localStorage behavior' } }],
        usage: null,
        stopReason: null,
      },
    } satisfies import('#renderer/global/types').TranscriptEvent;

    assert.match(transcriptEventSearchText(event), /localStorage behavior/);
  });
});

describe('Codex transcript presentation helpers', () => {
  it('groups consecutive Codex commands across carrier and token events', () => {
    const firstTool = bashTool('tool-1', 'pwd');
    const secondTool = bashTool('tool-2', 'git status');
    const events: TranscriptEvent[] = [
      assistantEvent(1, { kind: 'thinking', data: { text: 'Inspecting the repository' } }),
      assistantEvent(2, { kind: 'tool_use', data: firstTool }),
      {
        kind: 'user',
        data: {
          meta: eventMeta(3, 'result-1'),
          blocks: [
            {
              kind: 'tool_result',
              data: {
                toolUseId: firstTool.id,
                content: { kind: 'text', data: { text: '/tmp/project' } },
                isError: false,
              },
            },
          ],
        },
      },
      {
        kind: 'system_token_usage',
        data: {
          meta: eventMeta(4, 'usage-1'),
          usage: {
            input_tokens: 10,
            output_tokens: 5,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
          },
          contextWindow: null,
        },
      },
      assistantEvent(5, { kind: 'tool_use', data: secondTool }),
      assistantEvent(6, { kind: 'thinking', data: { text: 'Planning the change' } }),
    ];

    const rows = buildTranscriptRenderRows(events);

    assert.deepEqual(
      rows.map((row) => row.kind),
      ['event', 'tool_group', 'event'],
    );
    const group = rows[1];
    assert.equal(group?.kind, 'tool_group');
    if (group?.kind !== 'tool_group') return;
    assert.deepEqual(
      group.group.tools.map((tool) => tool.id),
      ['tool-1', 'tool-2'],
    );
    assert.equal(group.searchEvents.length, 3);
  });

  it('leaves a single action visible and groups every tool kind within one assistant event', () => {
    const tool = bashTool('tool-1', 'pwd');
    assert.equal(
      buildTranscriptRenderRows([assistantEvent(1, { kind: 'tool_use', data: tool })])[0]?.kind,
      'event',
    );

    const items = groupAssistantBlocks(
      {
        meta: eventMeta(1, 'assistant-1'),
        model: null,
        blocks: [
          { kind: 'tool_use', data: tool },
          {
            kind: 'tool_use',
            data: {
              id: 'tool-2',
              name: { kind: 'write' },
              input: { kind: 'write', data: { filePath: 'README.md', content: 'hello' } },
            },
          },
        ],
        usage: null,
        stopReason: null,
      },
      1,
    );
    assert.equal(items[0]?.kind, 'group');
  });

  it('uses thoughts as hard boundaries between activity groups', () => {
    const rows = buildTranscriptRenderRows([
      assistantEvent(1, { kind: 'tool_use', data: bashTool('tool-1', 'pwd') }),
      assistantEvent(2, { kind: 'tool_use', data: bashTool('tool-2', 'git status') }),
      assistantEvent(3, { kind: 'thinking', data: { text: 'Planning the change' } }),
      assistantEvent(4, { kind: 'tool_use', data: bashTool('tool-3', 'npm test') }),
    ]);

    assert.deepEqual(
      rows.map((row) => row.kind),
      ['tool_group', 'event', 'event'],
    );
  });

  it('summarizes grouped activity with natural action counts', () => {
    assert.equal(
      toolGroupSummary([bashTool('tool-1', 'pwd'), bashTool('tool-2', 'git status')]),
      'Ran 2 commands',
    );
  });

  it('removes reasoning markdown and exact duplicated summaries', () => {
    assert.equal(cleanThinkingText('**Inspecting files****Inspecting files**'), 'Inspecting files');
    assert.equal(cleanThinkingText('**Planning the change**'), 'Planning the change');
    assert.equal(
      cleanThinkingText('**Inspecting persistence****Detailing filters**'),
      'Inspecting persistence · Detailing filters',
    );
  });

  it('removes the app-server shell launcher from command summaries', () => {
    assert.equal(
      bashSummary(`/bin/zsh -lc 'npm run lint && npm run build'`),
      'npm run lint && npm run build',
    );
  });

  it('summarizes typed file changes and classifies diff lines', () => {
    const input: FileChangeInput = {
      changes: [{ path: 'src/App.tsx', kind: 'update', movePath: null, diff: '-old\n+new' }],
    };
    assert.equal(fileChangeSummary(input), 'Edited src/App.tsx');
    assert.equal(diffLineTone('+new'), 'text-success');
    assert.equal(diffLineTone('-old'), 'text-destructive');
    assert.equal(compactDisplayPath('/tmp/sandbox/plans/2/workpad.md'), '…/plans/2/workpad.md');
  });

  it('recognizes only Mando captain task handoffs as injected task instructions', () => {
    assert.equal(
      isMandoTaskPromptBody(
        'Read /tmp/task/.ai/briefs/todo-2.md and implement the task described there.\nRead /tmp/plans/2/workpad.md before writing to it.',
      ),
      true,
    );
    assert.equal(isMandoTaskPromptBody('Read the brief and explain it to me.'), false);
    assert.equal(
      mandoPromptLabel('You are the task clarifier. Research first.'),
      'Clarifier instructions',
    );
    assert.equal(mandoPromptLabel('You are the person using this app.'), null);
  });
});
