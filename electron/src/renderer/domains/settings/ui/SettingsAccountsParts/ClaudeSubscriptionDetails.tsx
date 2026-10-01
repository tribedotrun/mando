import React from 'react';
import { RefreshCw } from 'lucide-react';
import { useClaudeSubscription } from '#renderer/domains/settings/runtime/useClaudeSubscription';
import { Button } from '#renderer/global/ui/primitives/button';
import type { CredentialInfo } from '#shared/daemon-contract';

const DAY_MS = 86_400_000;

function formatSubscriptionDate(timestamp: number): string {
  return new Date(timestamp).toLocaleString([], {
    year: 'numeric',
    month: 'short',
    day: 'numeric',
    hour: 'numeric',
    minute: '2-digit',
    timeZoneName: 'short',
  });
}

export function ClaudeSubscriptionDetails({ cred }: { cred: CredentialInfo }): React.ReactElement {
  const { profile, refresh, refreshPlan, authorize, authorizing, allowKeychainAccess } =
    useClaudeSubscription(cred.id);
  const saved = cred.claudeSubscription;
  const subscription = profile.data?.state === 'ready' ? saved : null;
  const configured = profile.data && profile.data.state !== 'unconfigured';
  const checkedAt = subscription?.checkedAt;
  const stale = checkedAt != null && Date.now() - checkedAt >= DAY_MS;
  const authorizationRecovered = checkedAt != null && checkedAt >= authorize.submittedAt;
  const refreshRecovered = checkedAt != null && checkedAt >= refresh.submittedAt;
  const authorizationError = authorizationRecovered
    ? null
    : (authorize.error?.message ??
      (authorize.data?.authorized === false
        ? 'Keychain access was denied. Try again to allow access.'
        : null));
  const refreshError = refreshRecovered ? null : refresh.error?.message;
  const error = authorizationError ?? refreshError ?? saved?.error;
  const failedAt = authorizationError
    ? authorize.submittedAt
    : refreshError
      ? refresh.submittedAt
      : saved?.attemptedAt;
  const endsAt = subscription?.endsAt;
  const renewsAt = subscription?.renewsAt;

  return (
    <div className="mt-2 space-y-1 text-xs" data-testid={`claude-subscription-${cred.id}`}>
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
          <span className="font-medium text-muted-foreground">Plan</span>
          {subscription?.planName ? (
            <>
              <span className="font-medium text-foreground">{subscription.planName}</span>
              {subscription.billingInterval ? (
                <span className="text-muted-foreground">{subscription.billingInterval}</span>
              ) : null}
              {subscription.status && subscription.status !== 'active' ? (
                <span className="text-warning">{subscription.status}</span>
              ) : null}
            </>
          ) : (
            <span className="text-muted-foreground">
              {checkedAt != null
                ? 'Plan unavailable from Desktop billing.'
                : profile.data?.state === 'account_changed'
                  ? 'Unavailable. Restore the linked Desktop account below.'
                  : profile.data?.state === 'login_required'
                    ? 'Unavailable. Sign in to Desktop below.'
                    : configured
                      ? 'Not checked yet. Refresh using the Desktop login.'
                      : profile.isLoading
                        ? 'Checking Desktop profile…'
                        : 'Unavailable. Set up or link a Desktop profile below.'}
            </span>
          )}
        </div>
        <Button
          size="xs"
          variant="ghost"
          data-testid={`claude-subscription-refresh-${cred.id}`}
          aria-label={`Refresh plan for ${cred.label}`}
          disabled={
            refresh.isPending ||
            authorizing ||
            !configured ||
            profile.data?.state === 'account_changed'
          }
          onClick={refreshPlan}
        >
          <RefreshCw size={12} className={refresh.isPending ? 'animate-spin' : undefined} />
          {refresh.isPending ? 'Refreshing…' : 'Refresh plan'}
        </Button>
      </div>
      {subscription?.keychainAccessRequired ? (
        <div className="space-y-1" data-testid={`claude-subscription-keychain-${cred.id}`}>
          <p className="text-muted-foreground">
            Allow Mando to read this profile’s login from Keychain. Automatic refreshes never open
            password dialogs.
          </p>
          <p className="text-muted-foreground">
            Choose “Always Allow” in the macOS dialog when offered to remember access.
          </p>
          <Button
            size="xs"
            variant="outline"
            data-testid={`claude-subscription-authorize-keychain-${cred.id}`}
            disabled={authorizing || refresh.isPending}
            onClick={allowKeychainAccess}
          >
            {authorizing ? 'Waiting for Keychain…' : 'Allow Keychain access'}
          </Button>
        </div>
      ) : null}
      {endsAt != null ? (
        <p className="text-warning" data-testid={`claude-subscription-ends-${cred.id}`}>
          Ends {formatSubscriptionDate(endsAt)}
        </p>
      ) : subscription?.endsBefore ? (
        <p className="text-warning" data-testid={`claude-subscription-ends-${cred.id}`}>
          Ends before {subscription.endsBefore}
        </p>
      ) : renewsAt != null ? (
        <p className="text-muted-foreground" data-testid={`claude-subscription-renews-${cred.id}`}>
          Renews {formatSubscriptionDate(renewsAt)}
        </p>
      ) : subscription?.nextChargeDate ? (
        <p className="text-muted-foreground" data-testid={`claude-subscription-renews-${cred.id}`}>
          Renews {subscription.nextChargeDate}
        </p>
      ) : null}
      {checkedAt != null ? (
        <p className="text-muted-foreground" data-testid={`claude-subscription-checked-${cred.id}`}>
          Last checked {formatSubscriptionDate(checkedAt)}
          {stale ? <span className="ml-2 text-warning">Stale</span> : null}
        </p>
      ) : null}
      {error ? (
        <p
          role="alert"
          className="break-words text-destructive"
          data-testid={`claude-subscription-error-${cred.id}`}
        >
          {authorizationError ? 'Keychain access failed' : 'Plan refresh failed'}: {error}
          {failedAt != null ? (
            <span className="block">Last attempt {formatSubscriptionDate(failedAt)}</span>
          ) : null}
          {checkedAt != null ? (
            <span className="block">Showing the last successful plan details.</span>
          ) : null}
        </p>
      ) : refresh.isSuccess && checkedAt != null && checkedAt >= refresh.submittedAt ? (
        <p role="status" className="text-muted-foreground">
          Plan refreshed.
        </p>
      ) : null}
      {profile.error ? (
        <p role="alert" className="text-destructive">
          Could not check Desktop profile: {profile.error.message}
        </p>
      ) : null}
    </div>
  );
}
