import React, { useState, useCallback } from 'react';
import { useMountEffect } from '#renderer/global/runtime/useMountEffect';
import {
  SetupLayout,
  GhostButton,
  OutlineButton,
  PrimaryButton,
} from '#renderer/domains/onboarding/ui/OnboardingPrimitives';

type CCResult = {
  installed: boolean;
  version: string | null;
  works: boolean;
  checkFailed?: boolean;
  error?: string;
} | null;

export function ClaudeCheckScreen({
  onBack,
  onPass,
  checkClaudeCode,
}: {
  onBack: () => void;
  onPass: () => void;
  checkClaudeCode: () => Promise<CCResult>;
}): React.ReactElement {
  const [checking, setChecking] = useState(false);
  const [result, setResult] = useState<CCResult>(null);

  const runCheck = useCallback(async () => {
    setChecking(true);
    setResult(null);
    try {
      setResult(await checkClaudeCode());
    } finally {
      setChecking(false);
    }
  }, [checkClaudeCode]);

  useMountEffect(() => {
    void runCheck();
  });

  const passed = result?.installed === true && result.works === true;

  return (
    <SetupLayout
      data-testid="onboarding-wizard"
      step={1}
      total={2}
      title="Claude Code"
      subtitle="Required to run Mando."
    >
      <div className="mb-10 flex flex-col gap-3 rounded-lg bg-muted px-7 py-7 shadow-sm">
        {checking && (
          <div className="flex items-center gap-2.5">
            <span className="size-3.5 animate-spin shrink-0 rounded-full border-2 border-foreground border-t-transparent" />
            <span className="text-body text-muted-foreground">Checking...</span>
          </div>
        )}
        {result && !checking && result.checkFailed && (
          <StatusCheck
            ok={false}
            label={`Check failed: ${result.error ?? 'Unknown error'} \u2014 retry`}
          />
        )}
        {result && !checking && !result.checkFailed && (
          <>
            <StatusCheck ok={result.installed} label="Installed" detail={result.version} />
            {result.installed && (
              <StatusCheck
                ok={result.works}
                label={result.works ? 'Responding' : 'Not responding, check your API key'}
              />
            )}
          </>
        )}
      </div>

      {result && !result.checkFailed && !result.installed && !checking && (
        <p className="mb-6 text-body text-muted-foreground">
          Install Claude Code from{' '}
          <a
            href="https://code.claude.com/docs/en/overview"
            target="_blank"
            rel="noopener noreferrer"
            className="text-foreground"
          >
            the docs
          </a>
          , then re-check.
        </p>
      )}

      <div className="flex items-center justify-between">
        <div className="flex items-center gap-3">
          <GhostButton onClick={onBack}>Back</GhostButton>
          {!passed && (
            <OutlineButton onClick={() => void runCheck()} disabled={checking}>
              {checking ? 'Checking...' : 'Re-check'}
            </OutlineButton>
          )}
        </div>
        <PrimaryButton onClick={onPass} disabled={!passed}>
          Continue
        </PrimaryButton>
      </div>
    </SetupLayout>
  );
}

function StatusCheck({
  ok,
  label,
  detail,
}: {
  ok: boolean;
  label: string;
  detail?: string | null;
}): React.ReactElement {
  return (
    <div className="flex items-center gap-2.5">
      <span
        className={`flex size-4 shrink-0 items-center justify-center rounded-full text-[11px] font-bold text-background ${ok ? 'bg-success' : 'bg-destructive'}`}
      >
        {ok ? '\u2713' : '\u2717'}
      </span>
      <span className={`text-body ${ok ? 'text-foreground' : 'text-destructive'}`}>{label}</span>
      {detail && <span className="text-caption text-muted-foreground">{detail}</span>}
    </div>
  );
}
