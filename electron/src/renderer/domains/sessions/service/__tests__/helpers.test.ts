import assert from 'node:assert/strict';
import { describe, it } from 'node:test';

import { buildResumeCmd } from '../resumeCommand.ts';
import {
  bashSummary,
  cleanThinkingText,
  compactDisplayPath,
  diffLineTone,
  extractToolResultText,
  fileChangeSummary,
  toolGroupSummary,
} from '../transcriptRenderHelpers.ts';
import {
  buildProviderTranscriptRenderRows,
  buildTranscriptRenderRows,
  groupAssistantBlocks,
  isMandoTaskPromptBody,
  mandoPromptLabel,
  parseClaudeRateLimitInfo,
  resolveActiveBranch,
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
      messageId: null,
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
        messageId: null,
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
        messageId: null,
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

describe('Claude transcript presentation helpers', () => {
  it('renders tool-result images instead of a placeholder string', () => {
    assert.equal(
      extractToolResultText({
        toolUseId: 'tool-1',
        content: {
          kind: 'blocks',
          data: {
            blocks: [
              { kind: 'image', data: { mediaType: 'image/png', dataLen: 123 } },
              { kind: 'text', data: { text: 'caption' } },
            ],
          },
        },
        isError: false,
      }),
      'caption',
    );
  });

  it('hides Claude image sizing carrier messages from the transcript', () => {
    const imageMetadata: TranscriptEvent = {
      kind: 'user',
      data: {
        meta: eventMeta(2, 'image-metadata'),
        blocks: [
          {
            kind: 'text',
            data: {
              text: '[Image: original 1290x2796, displayed at 923x2000. Multiply coordinates by 1.40 to map to original image.]',
            },
          },
        ],
      },
    };

    assert.deepEqual(buildProviderTranscriptRenderRows([imageMetadata], 'claude'), []);
    assert.equal(buildProviderTranscriptRenderRows([imageMetadata], 'codex').length, 1);
  });

  it('drops empty signed thinking blocks instead of rendering a brain-only row', () => {
    const event = assistantEvent(1, {
      kind: 'thinking',
      data: { text: '  ' },
    });
    if (event.kind !== 'assistant') return;
    event.data.blocks.push(
      { kind: 'tool_use', data: bashTool('tool-1', 'pwd') },
      { kind: 'thinking', data: { text: '' } },
      { kind: 'tool_use', data: bashTool('tool-2', 'git status') },
    );

    const items = groupAssistantBlocks(event.data, 1);

    assert.equal(items.length, 1);
    assert.equal(items[0]?.kind, 'group');
    if (items[0]?.kind !== 'group') return;
    assert.equal(items[0].group.tools.length, 2);
  });

  it('reassembles content-block envelopes by Claude API message id', () => {
    const thinking = assistantEvent(1, { kind: 'thinking', data: { text: 'Inspecting' } });
    const tool = assistantEvent(2, {
      kind: 'tool_use',
      data: bashTool('tool-1', 'git status'),
    });
    if (thinking.kind !== 'assistant' || tool.kind !== 'assistant') return;
    thinking.data.messageId = 'msg-1';
    tool.data.messageId = 'msg-1';

    const active = resolveActiveBranch([thinking, tool], 'claude');

    assert.equal(active.length, 1);
    assert.equal(active[0]?.kind, 'assistant');
    if (active[0]?.kind !== 'assistant') return;
    assert.deepEqual(
      active[0].data.blocks.map((block) => block.kind),
      ['thinking', 'tool_use'],
    );
  });

  it('does not apply Codex cross-message activity grouping to Claude', () => {
    const rows = buildProviderTranscriptRenderRows(
      [
        assistantEvent(1, { kind: 'tool_use', data: bashTool('tool-1', 'pwd') }),
        assistantEvent(2, { kind: 'tool_use', data: bashTool('tool-2', 'git status') }),
      ],
      'claude',
    );
    assert.deepEqual(
      rows.map((row) => row.kind),
      ['event', 'event'],
    );
  });

  it('parses every Fable quota window and overage signal', () => {
    const info = parseClaudeRateLimitInfo(
      JSON.stringify({
        status: 'allowed',
        resetsAt: 1788339600,
        rateLimitType: 'five_hour',
        overageStatus: 'rejected',
        overageDisabledReason: 'org_level_disabled',
        isUsingOverage: false,
        unifiedWindows: {
          five_hour: { utilization: 0.11, resetsAt: 1788339600 },
          seven_day: { utilization: 0.05, resetsAt: 1788364800 },
        },
      }),
    );

    assert.equal(info?.status, 'allowed');
    assert.equal(info?.overageDisabledReason, 'org_level_disabled');
    assert.deepEqual(
      info?.windows.map((window) => [window.name, window.utilization]),
      [
        ['five_hour', 0.11],
        ['seven_day', 0.05],
      ],
    );
  });
});
