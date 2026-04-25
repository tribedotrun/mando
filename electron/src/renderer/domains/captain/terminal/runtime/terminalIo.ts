import type { Terminal as XTerm } from '@xterm/xterm';
import { writeTerminalBytes, resizeTerminal } from '#renderer/domains/captain/repo/terminal-api';
import { writeWithCallback } from '#renderer/domains/captain/terminal/runtime/terminalConfig';
import log from '#renderer/global/service/logger';

export async function writeTerminalBytesLogged(
  sessionId: string,
  bytes: Uint8Array,
  failureMessage: string,
): Promise<void> {
  try {
    await writeTerminalBytes(sessionId, bytes);
  } catch (err) {
    log.warn(failureMessage, err);
  }
}

export async function writeTerminalOutputLogged(
  term: XTerm,
  data: string | Uint8Array,
  failureMessage: string,
): Promise<void> {
  try {
    await writeWithCallback(term, data);
  } catch (err) {
    log.warn(failureMessage, err);
  }
}

export async function resizeTerminalLogged(
  sessionId: string,
  rows: number,
  cols: number,
): Promise<void> {
  try {
    await resizeTerminal(sessionId, rows, cols);
  } catch (err) {
    log.warn('Terminal resize failed', err);
  }
}
