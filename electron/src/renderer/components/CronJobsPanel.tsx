import React, { useState } from 'react';
import { useMountEffect } from '#renderer/hooks/useMountEffect';
import { useCronStore } from '#renderer/stores/cronStore';
import { useToastStore } from '#renderer/stores/toastStore';
import log from '#renderer/logger';
import type { CronJob } from '#renderer/types';
import { relativeTime } from '#renderer/utils';

type Variant = 'card' | 'settings' | 'page';

interface Props {
  variant: Variant;
  testId: string;
}

function inputStyle(variant: Variant): React.CSSProperties {
  return {
    border: '1px solid var(--color-border-subtle)',
    background: variant === 'card' ? 'var(--color-surface-1)' : 'var(--color-surface-2)',
    color: 'var(--color-text-1)',
    borderRadius: 'var(--radius-button)',
    padding: '6px 12px',
    fontSize: 14,
  };
}

function AddJobForm({
  onDone,
  variant,
}: {
  onDone: () => void;
  variant: Variant;
}): React.ReactElement {
  const add = useCronStore((s) => s.add);
  const [name, setName] = useState('');
  const [scheduleKind, setScheduleKind] = useState<CronJob['schedule_kind']>('every');
  const [scheduleValue, setScheduleValue] = useState('');
  const [message, setMessage] = useState('');
  const [submitting, setSubmitting] = useState(false);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!name.trim() || !scheduleValue.trim() || !message.trim()) return;
    setSubmitting(true);
    try {
      await add({
        name: name.trim(),
        schedule_kind: scheduleKind,
        schedule_value: scheduleValue.trim(),
        message: message.trim(),
      });
      setName('');
      setScheduleValue('');
      setMessage('');
      onDone();
    } catch {
      // Error already set in cronStore
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <form
      onSubmit={handleSubmit}
      className="space-y-2"
      style={{
        marginBottom: 16,
        padding: 16,
        borderRadius: 'var(--radius-panel)',
        background: variant === 'card' ? 'var(--color-surface-2)' : 'var(--color-surface-1)',
        border: '1px solid var(--color-border-subtle)',
      }}
    >
      <div className="flex gap-2">
        <input
          type="text"
          placeholder="Job name"
          value={name}
          onChange={(e) => setName(e.target.value)}
          className="flex-1 focus:outline-none"
          style={inputStyle(variant)}
        />
        <select
          value={scheduleKind}
          onChange={(e) => setScheduleKind(e.target.value as CronJob['schedule_kind'])}
          className="focus:outline-none"
          style={inputStyle(variant)}
        >
          <option value="every">every</option>
          <option value="at">at</option>
          <option value="cron">cron</option>
        </select>
        <input
          type="text"
          placeholder="Schedule value"
          value={scheduleValue}
          onChange={(e) => setScheduleValue(e.target.value)}
          className="focus:outline-none"
          style={{ ...inputStyle(variant), width: 120 }}
        />
      </div>
      <div className="flex gap-2">
        <input
          type="text"
          placeholder="Message to deliver"
          value={message}
          onChange={(e) => setMessage(e.target.value)}
          className="flex-1 focus:outline-none"
          style={inputStyle(variant)}
        />
        <button
          type="submit"
          disabled={!name.trim() || !scheduleValue.trim() || !message.trim() || submitting}
          className="text-[13px] font-medium disabled:opacity-50"
          style={{
            padding: variant === 'card' ? '6px 12px' : '6px 16px',
            borderRadius: 'var(--radius-button)',
            background: 'var(--color-accent)',
            color: 'var(--color-bg)',
            border: 'none',
            cursor: 'pointer',
          }}
        >
          {submitting ? '...' : 'Add'}
        </button>
        <button
          type="button"
          onClick={onDone}
          className="text-[13px]"
          style={{
            color: 'var(--color-text-2)',
            background: 'none',
            border: 'none',
            cursor: 'pointer',
          }}
        >
          Cancel
        </button>
      </div>
    </form>
  );
}

function JobRow({ job, variant }: { job: CronJob; variant: Variant }): React.ReactElement {
  const toggle = useCronStore((s) => s.toggle);
  const runNow = useCronStore((s) => s.runNow);
  const remove = useCronStore((s) => s.remove);
  const [running, setRunning] = useState(false);
  const [confirming, setConfirming] = useState(false);

  const handleRun = async () => {
    setRunning(true);
    try {
      await runNow(job.id);
    } catch {
      // Error already set in cronStore
    } finally {
      setRunning(false);
    }
  };

  const handleRemove = async () => {
    try {
      await remove(job.id);
    } catch (err) {
      log.error('[CronJobsPanel] remove failed:', err);
      useToastStore.getState().add('error', 'Failed to remove cron job');
    } finally {
      setConfirming(false);
    }
  };

  return (
    <div
      className={
        variant === 'card'
          ? 'flex items-center justify-between rounded px-3 py-2'
          : 'flex items-center justify-between'
      }
      style={{
        background: variant === 'card' ? 'var(--color-surface-2)' : 'transparent',
        padding: variant === 'card' ? undefined : '10px 0',
        minHeight: variant === 'card' ? undefined : 40,
      }}
    >
      <div className="min-w-0 flex-1">
        <span
          className={variant === 'card' ? 'text-sm' : 'text-body'}
          style={{ color: 'var(--color-text-1)' }}
        >
          {job.name}
        </span>
        <span
          className={variant === 'card' ? 'ml-2 text-xs' : 'text-caption'}
          style={{ color: 'var(--color-text-3)', marginLeft: variant === 'card' ? undefined : 8 }}
        >
          {job.schedule_kind}: {job.schedule_value}
        </span>
        {(job.last_run || job.next_run) && (
          <div
            className={variant === 'card' ? 'mt-1 flex gap-4 text-xs' : 'flex gap-4 text-caption'}
            style={{ color: 'var(--color-text-3)' }}
          >
            {job.last_run && <span>Last: {relativeTime(job.last_run)}</span>}
            {job.next_run && <span>Next: {relativeTime(job.next_run)}</span>}
          </div>
        )}
      </div>
      <div className="flex items-center gap-2">
        <button
          onClick={handleRun}
          disabled={running}
          className="text-[13px] font-medium text-white disabled:opacity-50"
          style={{
            padding: '4px 12px',
            borderRadius: 'var(--radius-button)',
            background: 'var(--color-accent)',
            color: 'var(--color-bg)',
            border: 'none',
            cursor: 'pointer',
          }}
        >
          {running ? '...' : 'Run'}
        </button>
        <button
          onClick={() => toggle(job.id, !job.enabled).catch(() => {})}
          className="text-[13px] transition-colors"
          style={{
            padding: '4px 12px',
            borderRadius: 'var(--radius-button)',
            background: job.enabled ? 'var(--color-success-bg)' : 'var(--color-surface-3)',
            color: job.enabled ? 'var(--color-success)' : 'var(--color-text-2)',
            fontWeight: 500,
            border: 'none',
            cursor: 'pointer',
          }}
        >
          {job.enabled ? 'ON' : 'OFF'}
        </button>
        {confirming ? (
          <>
            <button
              onClick={handleRemove}
              className="text-[13px]"
              style={{
                padding: '4px 12px',
                borderRadius: 'var(--radius-button)',
                background: 'var(--color-error)',
                color: '#fff',
                fontWeight: 500,
                border: 'none',
                cursor: 'pointer',
              }}
            >
              Confirm
            </button>
            <button
              onClick={() => setConfirming(false)}
              className="text-[13px]"
              style={{
                color: 'var(--color-text-2)',
                background: 'none',
                border: 'none',
                cursor: 'pointer',
              }}
            >
              No
            </button>
          </>
        ) : (
          <button
            onClick={() => setConfirming(true)}
            className="text-[13px]"
            style={{
              padding: '4px 12px',
              borderRadius: 'var(--radius-button)',
              background: 'var(--color-surface-3)',
              color: 'var(--color-error)',
              fontWeight: 500,
              border: 'none',
              cursor: 'pointer',
            }}
          >
            Del
          </button>
        )}
      </div>
    </div>
  );
}

export function CronJobsPanel({ variant, testId }: Props): React.ReactElement {
  const { jobs, count, loading, error, fetch } = useCronStore();
  const [showForm, setShowForm] = useState(false);

  useMountEffect(() => {
    fetch();
  });

  if (variant === 'card') {
    return (
      <div
        data-testid={testId}
        className="rounded-lg border p-4"
        style={{ borderColor: 'var(--color-border)', background: 'var(--color-surface-1)' }}
      >
        <div className="mb-3 flex items-center justify-between">
          <h3 className="text-sm font-semibold" style={{ color: 'var(--color-text-1)' }}>
            Cron Jobs ({count})
          </h3>
          {!showForm && (
            <button
              onClick={() => setShowForm(true)}
              className="rounded px-2 py-1 text-xs font-medium text-white"
              style={{ background: 'var(--color-accent)' }}
            >
              + Add
            </button>
          )}
        </div>
        {showForm && <AddJobForm variant={variant} onDone={() => setShowForm(false)} />}
        {loading && (
          <p className="text-sm" style={{ color: 'var(--color-text-3)' }}>
            Loading...
          </p>
        )}
        {error && (
          <p className="text-sm" style={{ color: 'var(--color-error)' }}>
            {error}
          </p>
        )}
        {!loading && jobs.length === 0 && (
          <p className="text-sm" style={{ color: 'var(--color-text-3)' }}>
            No cron jobs configured.
          </p>
        )}
        <div className="space-y-2">
          {jobs.map((job) => (
            <JobRow key={job.id} job={job} variant={variant} />
          ))}
        </div>
      </div>
    );
  }

  return (
    <div data-testid={testId}>
      <div className="flex items-center justify-between" style={{ marginBottom: 24 }}>
        <h2 className="text-heading" style={{ color: 'var(--color-text-1)' }}>
          Scheduled Tasks
        </h2>
        {!showForm && (
          <button
            onClick={() => setShowForm(true)}
            className="text-[13px] font-medium"
            style={{
              padding: '8px 20px',
              borderRadius: 'var(--radius-button)',
              border: '1px solid var(--color-border)',
              background: 'transparent',
              color: 'var(--color-text-2)',
              cursor: 'pointer',
            }}
          >
            + Add Job
          </button>
        )}
      </div>
      {showForm && <AddJobForm variant={variant} onDone={() => setShowForm(false)} />}
      {loading && (
        <p className="text-body" style={{ color: 'var(--color-text-3)' }}>
          Loading...
        </p>
      )}
      {error && (
        <p className="text-body" style={{ color: 'var(--color-error)' }}>
          {error}
        </p>
      )}
      {!loading && jobs.length === 0 && !showForm && (
        <p className="text-body" style={{ color: 'var(--color-text-3)' }}>
          No scheduled tasks configured.
        </p>
      )}
      <div>
        {jobs.map((job) => (
          <JobRow key={job.id} job={job} variant={variant} />
        ))}
      </div>
    </div>
  );
}
