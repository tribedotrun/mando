import assert from 'node:assert/strict';
import { describe, it } from 'node:test';
import {
  formatExactTokenCount,
  formatUsageBreakdown,
  summarizeTranscriptTokenUsage,
} from '../transcriptTokenUsage.ts';
import type { EventMeta, TranscriptEvent, TranscriptUsageInfo } from '#renderer/global/types';

function meta(line: number): EventMeta {
  return {
    index: { lineNumber: line },
    uuid: null,
    parentUuid: null,
    sessionId: 'session-1',
    timestamp: null,
    isSidechain: null,
  };
}

function usage(
  input: number,
  output: number,
  cacheRead = 0,
  cacheCreation = 0,
): TranscriptUsageInfo {
  return {
    input_tokens: input,
    output_tokens: output,
    cache_read_tokens: cacheRead,
    cache_creation_tokens: cacheCreation,
  };
}

describe('summarizeTranscriptTokenUsage', () => {
  it('uses the final result summary as the authoritative completed-session total', () => {
    const events: TranscriptEvent[] = [
      {
        kind: 'assistant',
        data: {
          meta: meta(1),
          model: 'claude',
          blocks: [],
          usage: usage(1, 1),
          stopReason: null,
        },
      },
      {
        kind: 'result',
        data: {
          meta: meta(2),
          outcome: 'success',
          summary: {
            durationMs: null,
            durationApiMs: null,
            numTurns: null,
            totalCostUsd: null,
            stopReason: null,
            permissionDenials: [],
            errors: [],
            usage: usage(700, 30, 20, 10),
            modelUsage: [],
            isError: false,
          },
        },
      },
    ];

    assert.deepEqual(summarizeTranscriptTokenUsage(events), {
      input_tokens: 700,
      output_tokens: 30,
      cache_read_tokens: 20,
      cache_creation_tokens: 10,
      totalTokens: 760,
      source: 'result',
    });
  });

  it('reads exact live Codex thread token usage from its typed progress event', () => {
    const events: TranscriptEvent[] = [
      {
        kind: 'system_token_usage',
        data: {
          meta: meta(1),
          usage: usage(7, 3, 2, 1),
          contextWindow: 200000,
        },
      },
    ];

    assert.deepEqual(summarizeTranscriptTokenUsage(events), {
      input_tokens: 7,
      output_tokens: 3,
      cache_read_tokens: 2,
      cache_creation_tokens: 1,
      totalTokens: 13,
      source: 'system_token_usage',
    });
  });

  it('prefers exact Codex token usage updates over result usage that cannot carry totalTokens', () => {
    const events: TranscriptEvent[] = [
      {
        kind: 'unknown',
        data: {
          meta: meta(1),
          rawType: 'thread/tokenUsage/updated',
          rawSubtype: null,
          raw: JSON.stringify({
            method: 'thread/tokenUsage/updated',
            params: {
              tokenUsage: {
                total: {
                  totalTokens: 227891,
                  inputTokens: 226612,
                  outputTokens: 1279,
                  cachedInputTokens: 166016,
                },
              },
            },
          }),
        },
      },
      {
        kind: 'result',
        data: {
          meta: meta(2),
          outcome: 'success',
          summary: {
            durationMs: null,
            durationApiMs: null,
            numTurns: null,
            totalCostUsd: null,
            stopReason: null,
            permissionDenials: [],
            errors: [],
            usage: usage(226612, 1279, 166016),
            modelUsage: [],
            isError: false,
          },
        },
      },
    ];

    assert.equal(summarizeTranscriptTokenUsage(events)?.totalTokens, 227891);
  });

  it('falls back to summing assistant usage for active Claude sessions', () => {
    const events: TranscriptEvent[] = [
      {
        kind: 'assistant',
        data: {
          meta: meta(1),
          model: 'claude',
          blocks: [],
          usage: usage(10, 2, 5),
          stopReason: null,
        },
      },
      {
        kind: 'assistant',
        data: {
          meta: meta(2),
          model: 'claude',
          blocks: [],
          usage: usage(3, 4, 0, 1),
          stopReason: null,
        },
      },
    ];

    assert.deepEqual(summarizeTranscriptTokenUsage(events), {
      input_tokens: 13,
      output_tokens: 6,
      cache_read_tokens: 5,
      cache_creation_tokens: 1,
      totalTokens: 25,
      source: 'assistant_sum',
    });
  });

  it('formats exact totals and exposes the full token breakdown', () => {
    assert.equal(formatExactTokenCount(4313160), '4,313,160');
    assert.equal(
      formatUsageBreakdown(usage(700, 30, 20, 10)),
      'input 700 · output 30 · cache read 20 · cache write 10',
    );
  });
});
