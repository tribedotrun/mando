import React from 'react';

export function CodexAuthCaptureHelp(): React.ReactElement {
  return (
    <div className="space-y-1.5 text-xs text-muted-foreground/70">
      <p>
        Run:{' '}
        <code className="rounded bg-muted px-1 py-0.5">
          d=$(mktemp -d); CODEX_HOME=&quot;$d&quot; codex login
        </code>
      </p>
      <p>
        Then pick or paste{' '}
        <code className="rounded bg-muted px-1 py-0.5">&quot;$d/auth.json&quot;</code> below. Delete
        the directory afterwards, and never run{' '}
        <code className="rounded bg-muted px-1 py-0.5">codex logout</code> in it (logout revokes the
        captured session).
      </p>
      <p>
        Adding your personal <code className="rounded bg-muted px-1 py-0.5">~/.codex</code> account
        here is fine as a separate session, but it shares that account&apos;s rate limits with your
        personal use.
      </p>
    </div>
  );
}
