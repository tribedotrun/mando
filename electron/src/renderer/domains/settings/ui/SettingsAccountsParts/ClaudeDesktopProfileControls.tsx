import React, { useState } from 'react';
import { useClaudeDesktopProfile } from '#renderer/domains/settings/runtime/useClaudeDesktopProfile';
import { Button } from '#renderer/global/ui/primitives/button';
import { Input } from '#renderer/global/ui/primitives/input';
import type { ClaudeDesktopSessionImportRequest } from '#shared/daemon-contract';

export function ClaudeDesktopProfileControls({
  credentialId,
}: {
  credentialId: number;
}): React.ReactElement {
  const { status, pending, setup, open, adopt, importSessions, previewSessions } =
    useClaudeDesktopProfile(credentialId);
  const [showDetails, setShowDetails] = useState(false);
  const [showAdopt, setShowAdopt] = useState(false);
  const [existingPath, setExistingPath] = useState('');
  const [range, setRange] = useState<ClaudeDesktopSessionImportRequest['range']>('month');
  const currentPreview =
    previewSessions.variables?.range === range ? previewSessions.data : undefined;
  const profile = status.data;
  const configured = profile && profile.state !== 'unconfigured';
  const stateLabel = !profile
    ? 'Loading…'
    : {
        unconfigured: 'Not set up',
        login_required: 'Sign-in needed',
        ready: 'Account recorded',
        account_changed: 'Account changed',
      }[profile.state];

  return (
    <div className="mt-3 border-t border-border pt-3">
      <div className="flex flex-wrap items-center gap-2">
        <span className="text-xs font-medium text-foreground">Claude Desktop</span>
        <span className="text-xs text-muted-foreground">
          {stateLabel}
          {profile?.running ? ' · Running' : ''}
        </span>
        <div className="ml-auto flex items-center gap-1">
          {configured ? (
            <Button
              size="xs"
              variant="outline"
              disabled={pending || profile.state === 'account_changed'}
              onClick={() => open.mutate(credentialId)}
            >
              {profile.running ? 'Focus Desktop' : 'Open Desktop'}
            </Button>
          ) : (
            <Button
              size="xs"
              variant="outline"
              disabled={pending || !profile}
              onClick={() => setup.mutate(credentialId)}
            >
              Set up Desktop
            </Button>
          )}
          <Button
            size="xs"
            variant="ghost"
            onClick={() => setShowDetails(!showDetails)}
            aria-expanded={showDetails}
          >
            Manage
          </Button>
        </div>
      </div>
      {status.error ? (
        <p role="alert" className="mt-2 text-xs text-destructive">
          {status.error.message}
        </p>
      ) : null}
      {profile?.state === 'login_required' ? (
        <p className="mt-2 text-xs text-muted-foreground">
          Complete sign-in in Claude Desktop, then open it here to record the account.
        </p>
      ) : null}
      {profile?.state === 'account_changed' ? (
        <p role="alert" className="mt-2 text-xs text-destructive">
          Desktop’s account differs from the saved account. Sign in to the original account before
          opening it here.
        </p>
      ) : null}
      {configured && showDetails ? (
        <div className="mt-2 space-y-2">
          <p className="text-xs text-muted-foreground">
            Import local sessions only when you choose. Opening Desktop does not change its session
            list. Cloud sessions stay with their account.
          </p>
          <div
            className="flex flex-wrap items-center gap-1"
            role="group"
            aria-label="Local session import range"
          >
            {(
              [
                { value: 'week', label: '7 days' },
                { value: 'month', label: '30 days' },
                { value: 'all', label: 'All local sessions' },
              ] as const
            ).map((option) => (
              <Button
                key={option.value}
                size="xs"
                variant={range === option.value ? 'secondary' : 'ghost'}
                aria-pressed={range === option.value}
                disabled={pending}
                onClick={() => {
                  setRange(option.value);
                  importSessions.reset();
                }}
              >
                {option.label}
              </Button>
            ))}
            <Button
              size="xs"
              variant="outline"
              disabled={pending || profile.state !== 'ready'}
              onClick={() => previewSessions.mutate({ credentialId, range })}
            >
              Preview import
            </Button>
          </div>
          {currentPreview ? (
            <div className="flex flex-wrap items-center gap-2">
              <span className="text-xs text-muted-foreground">
                {currentPreview.eligible} eligible; {currentPreview.skipped} existing or ineligible.
              </span>
              <Button
                size="xs"
                variant="outline"
                disabled={pending || profile.running || !currentPreview.eligible}
                onClick={() =>
                  importSessions.mutate(
                    { credentialId, range },
                    { onSuccess: () => previewSessions.reset() },
                  )
                }
              >
                Import sessions
              </Button>
            </div>
          ) : null}
          {profile.running ? (
            <p className="text-xs text-muted-foreground">
              Quit this Desktop instance to import session entries.
            </p>
          ) : null}
          {importSessions.data ? (
            <p role="status" className="text-xs text-muted-foreground">
              Imported {importSessions.data.imported}; skipped {importSessions.data.skipped}{' '}
              existing or ineligible sessions.
              {importSessions.data.journalPath ? (
                <span className="block break-all font-mono">
                  Import journal: {importSessions.data.journalPath}
                </span>
              ) : null}
            </p>
          ) : null}
        </div>
      ) : null}
      {showDetails ? (
        <div className="mt-2 space-y-2 text-xs text-muted-foreground">
          <p>
            All Desktop accounts share local Claude Code settings, memory, and transcripts. Desktop
            login is separate from the CLI token.
          </p>
          {profile ? (
            <>
              <p className="break-all">
                <span className="font-medium">Desktop folder: </span>
                <span className="font-mono">{profile.userDataDir}</span>
              </p>
              <p className="break-all">
                <span className="font-medium">Shared Code folder: </span>
                <span className="font-mono">{profile.sharedClaudeDir}</span>
              </p>
              {profile.accountUuid ? (
                <p className="break-all">
                  Desktop account ID: <span className="font-mono">{profile.accountUuid}</span>. This
                  is read from local Desktop state, not verified against the CLI token.
                </p>
              ) : null}
            </>
          ) : null}
          <Button
            size="xs"
            variant="ghost"
            onClick={() => setShowAdopt(!showAdopt)}
            aria-expanded={showAdopt}
          >
            Use an existing Desktop folder
          </Button>
          {showAdopt ? (
            <div className="space-y-2">
              <Input
                aria-label="Existing Claude Desktop user data directory"
                placeholder="/Users/you/Library/Application Support/Claude"
                value={existingPath}
                onChange={(event) => setExistingPath(event.target.value)}
              />
              <Button
                size="xs"
                variant="outline"
                disabled={pending || !existingPath.trim()}
                onClick={() => adopt.mutate({ credentialId, userDataDir: existingPath.trim() })}
              >
                Link folder
              </Button>
            </div>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}
