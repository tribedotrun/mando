import { useCallback, useState } from 'react';
import { apiPatchRouteR } from '#renderer/global/providers/http';
import { useConfigInvalidate } from '#renderer/domains/settings/runtime/hooks';
import { apiErrorMessage } from '#result';

/** Handles logo detection for a project via the API. */
export function useProjectLogo(projectName: string, initialLogo: string | null) {
  const invalidateConfig = useConfigInvalidate();
  const [logoFile, setLogoFile] = useState(initialLogo);
  const [detecting, setDetecting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const detectLogo = useCallback(async () => {
    setDetecting(true);
    setError(null);
    const result = await apiPatchRouteR(
      'patchProjectsByName',
      { redetect_logo: true },
      { params: { name: projectName } },
    );
    result.match(
      (res) => {
        setLogoFile(res.logo ?? null);
        invalidateConfig();
      },
      (err) => {
        setError(apiErrorMessage(err));
      },
    );
    setDetecting(false);
  }, [projectName, invalidateConfig]);

  return { logoFile, setLogoFile, detecting, error, detectLogo };
}
