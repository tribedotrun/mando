import assert from 'node:assert/strict';
import { afterEach, beforeEach, describe, it } from 'node:test';
import { formatWindowReset } from '../formatters.ts';

const REAL_NOW = Date.now;

function freezeNow(iso: string): void {
  const fixed = new Date(iso).getTime();
  Date.now = () => fixed;
}

function toSecs(iso: string): number {
  return Math.floor(new Date(iso).getTime() / 1000);
}

function expectedWeekday(iso: string): string {
  return new Date(iso).toLocaleDateString([], { weekday: 'short' });
}

function expectedMonthDay(iso: string): string {
  return new Date(iso).toLocaleDateString([], { month: 'short', day: 'numeric' });
}

describe('formatWindowReset date prefix', () => {
  beforeEach(() => {
    freezeNow('2026-04-22T10:00:00');
  });

  afterEach(() => {
    Date.now = REAL_NOW;
  });

  it('omits prefix when reset is later today', () => {
    const out = formatWindowReset(toSecs('2026-04-22T15:00:00'));
    assert.ok(!/^[A-Za-z]+ \d/.test(out), `expected no date prefix, got: ${out}`);
    assert.match(out, /\(in 5h\)$/);
  });

  it('omits prefix when both now and reset are late on the same night', () => {
    freezeNow('2026-04-22T23:30:00');
    const out = formatWindowReset(toSecs('2026-04-22T23:45:00'));
    assert.ok(!/^[A-Za-z]+ \d/.test(out), `expected no date prefix, got: ${out}`);
  });

  it('prepends short weekday when reset crosses midnight (<24h but next day)', () => {
    freezeNow('2026-04-22T23:00:00');
    const targetIso = '2026-04-23T01:00:00';
    const out = formatWindowReset(toSecs(targetIso));
    assert.ok(
      out.startsWith(`${expectedWeekday(targetIso)} `),
      `expected weekday prefix for ${targetIso}, got: ${out}`,
    );
  });

  it('prepends short weekday for a 3-day-out reset', () => {
    const targetIso = '2026-04-25T15:00:00';
    const out = formatWindowReset(toSecs(targetIso));
    assert.ok(
      out.startsWith(`${expectedWeekday(targetIso)} `),
      `expected weekday prefix for ${targetIso}, got: ${out}`,
    );
    assert.match(out, /\(in 3d/);
  });

  it('prepends MMM d for a 7-day-out reset', () => {
    const targetIso = '2026-04-29T15:00:00';
    const out = formatWindowReset(toSecs(targetIso));
    assert.ok(
      out.startsWith(`${expectedMonthDay(targetIso)} `),
      `expected "${expectedMonthDay(targetIso)} " prefix, got: ${out}`,
    );
    assert.match(out, /\(in 7d/);
  });

  it('returns "now" for a past reset', () => {
    assert.equal(formatWindowReset(toSecs('2026-04-22T09:00:00')), 'now');
  });

  it('returns "--" for null input', () => {
    assert.equal(formatWindowReset(null), '--');
  });

  it('derives the calendar-day baseline from Date.now, not the system clock', () => {
    // Historic frozen instant so the real wall clock can never accidentally
    // agree with it. If the formatter ever reverts to `new Date()` for its
    // calendar comparison, this assertion fails in CI because the real date
    // won't sit on 2020-01-15.
    freezeNow('2020-01-15T10:00:00');
    const targetIso = '2020-01-17T15:00:00';
    const out = formatWindowReset(toSecs(targetIso));
    assert.ok(
      out.startsWith(`${expectedWeekday(targetIso)} `),
      `expected weekday prefix anchored to frozen 2020-01-15, got: ${out}`,
    );
  });
});
