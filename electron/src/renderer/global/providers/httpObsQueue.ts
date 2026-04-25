// eslint-disable-next-line no-restricted-imports -- logger is cross-cutting infrastructure
import log from '#renderer/global/service/logger';
import { reportObsDegraded } from '#renderer/global/providers/obsHealth';
import {
  normalizeClientLogContext,
  type NormalizedClientLogContext,
} from '#shared/daemon-contract/clientLogs';
import { assertRouteBody, resolveRoutePath } from '#shared/daemon-contract/runtime';
import { buildUrl } from '#renderer/global/providers/httpBase';

function getErrorMessage(err: unknown, fallback: string): string {
  return err instanceof Error ? err.message : fallback;
}

interface ClientLogEntry {
  level: string;
  message: string;
  context: NormalizedClientLogContext | null;
  timestamp: string | null;
}

const MAX_ERROR_BATCH = 200;
const MAX_FLUSH_RETRIES = 5;
const BASE_RETRY_MS = 5_000;
const MAX_RETRY_MS = 60_000;
const FLUSH_DELAY_MS = 5_000;

function createHttpObsQueue() {
  let errorBatch: ClientLogEntry[] = [];
  let batchTimer: ReturnType<typeof setTimeout> | null = null;
  let flushFailures = 0;
  let degradationReported = false;

  async function flushErrors(): Promise<void> {
    batchTimer = null;
    if (errorBatch.length === 0) return;

    const entries = [...errorBatch];
    errorBatch = [];
    assertRouteBody('postClientlogs', { entries });

    try {
      await fetch(buildUrl(resolveRoutePath('postClientlogs')), {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ entries }),
      });
      flushFailures = 0;
    } catch (err) {
      flushFailures++;
      const reason = getErrorMessage(err, 'unknown');
      if (flushFailures >= MAX_FLUSH_RETRIES) {
        log.error(
          `[obs] dropping ${entries.length} entries after ${MAX_FLUSH_RETRIES} flush failures (last error: ${reason})`,
        );
        if (!degradationReported) {
          degradationReported = true;
          reportObsDegraded();
        }
        flushFailures = 0;
        return;
      }
      log.warn(
        `[obs] flush failed (attempt ${flushFailures}/${MAX_FLUSH_RETRIES}, ${reason}), will retry`,
      );
      errorBatch.push(...entries.slice(0, MAX_ERROR_BATCH - errorBatch.length));
      if (!batchTimer && errorBatch.length > 0) {
        const delay = Math.min(BASE_RETRY_MS * 2 ** flushFailures, MAX_RETRY_MS);
        batchTimer = setTimeout(() => void flushErrors(), delay);
      }
    }
  }

  return {
    getBatch(): readonly ClientLogEntry[] {
      return errorBatch;
    },
    clearBatch(): void {
      errorBatch = [];
    },
    queueError(level: string, message: string, context?: unknown): void {
      if (errorBatch.length >= MAX_ERROR_BATCH) return;
      errorBatch.push({
        level,
        message,
        context: normalizeClientLogContext(context),
        timestamp: new Date().toISOString(),
      });

      if (!batchTimer) {
        batchTimer = setTimeout(() => void flushErrors(), FLUSH_DELAY_MS);
      }
    },
  };
}

const httpObsQueue = createHttpObsQueue();

export function __testGetErrorBatch(): readonly ClientLogEntry[] {
  return httpObsQueue.getBatch();
}

export function __testClearErrorBatch(): void {
  httpObsQueue.clearBatch();
}

export function queueError(level: string, message: string, context?: unknown): void {
  httpObsQueue.queueError(level, message, context);
}
