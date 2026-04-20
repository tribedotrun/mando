import React, { useState } from 'react';
import { useMountEffect } from '#renderer/global/runtime/useMountEffect';
import log from '#renderer/global/service/logger';

/** Lazy onboarding entry so the wizard stays code-split until setup is needed. */
export function OnboardingPlaceholder(): React.ReactElement {
  const [OnboardingWizard, setOnboardingWizard] = useState<React.ComponentType | null>(null);
  const [loadError, setLoadError] = useState(false);

  useMountEffect(() => {
    import('#renderer/domains/onboarding/ui/OnboardingWizard')
      .then((mod) => {
        setOnboardingWizard(() => mod.OnboardingWizard);
      })
      .catch((err) => {
        log.error('[onboarding] chunk load failed:', err);
        setLoadError(true);
      });
  });

  if (loadError) {
    return <div className="p-6 text-destructive">Failed to load onboarding. Restart the app.</div>;
  }

  if (!OnboardingWizard) {
    return <div />;
  }

  return <OnboardingWizard />;
}
