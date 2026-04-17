import { useCallback, useState } from 'react';
import { apiPatch } from '#renderer/global/providers/http';
import { useConfigInvalidate } from '#renderer/domains/settings/runtime/hooks';

/** Handles logo detection for a project via the API. */
export function useProjectLogo(projectName: string, initialLogo: string | null) {
  const invalidateConfig = useConfigInvalidate();
  const [logoFile, setLogoFile] = useState(initialLogo);
  const [detecting, setDetecting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const detectLogo = useCallback(async () => {
    setDetecting(true);
    setError(null);
    try {
      const res = await apiPatch<{ logo?: string | null }>(
        `/api/projects/${encodeURIComponent(projectName)}`,
        { redetect_logo: true },
      );
      setLogoFile(res.logo ?? null);
      invalidateConfig();
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Detection failed');
    } finally {
      setDetecting(false);
    }
  }, [projectName, invalidateConfig]);

  return { logoFile, setLogoFile, detecting, error, detectLogo };
}
