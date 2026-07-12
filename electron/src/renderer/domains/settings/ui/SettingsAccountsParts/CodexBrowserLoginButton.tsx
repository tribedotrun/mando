import React from 'react';
import { ExternalLink, Loader2 } from 'lucide-react';
import { Button } from '#renderer/global/ui/primitives/button';
import { useFeedbackCodexLogin } from '#renderer/domains/settings/runtime/useFeedbackCodexLogin';

interface CodexBrowserLoginButtonProps {
  credentialId?: number;
  compact?: boolean;
}

export function CodexBrowserLoginButton({
  credentialId,
  compact,
}: CodexBrowserLoginButtonProps): React.ReactElement {
  const { flow, actions } = useFeedbackCodexLogin();
  const activeFlow =
    flow && flow.status === 'pending' && (flow.credentialId ?? null) === (credentialId ?? null)
      ? flow
      : null;

  if (!activeFlow) {
    return (
      <Button
        type="button"
        variant="outline"
        size="sm"
        onClick={() => actions.start({ credentialId })}
      >
        {compact ? 'Re-login in browser' : 'Sign in with ChatGPT'}
      </Button>
    );
  }

  return (
    <div className="flex flex-wrap items-center gap-2">
      <Button type="button" variant="outline" size="sm" disabled className="gap-1.5">
        <Loader2 size={14} className="animate-spin" />
        Waiting for browser sign-in...
      </Button>
      <Button type="button" variant="ghost" size="sm" onClick={actions.cancel}>
        Cancel
      </Button>
      {activeFlow.authUrl ? (
        <Button
          type="button"
          variant="link"
          size="sm"
          onClick={actions.openAuthUrl}
          className="gap-1"
        >
          <ExternalLink size={12} />
          Open sign-in link
        </Button>
      ) : null}
    </div>
  );
}
