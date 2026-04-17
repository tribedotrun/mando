import { useQuery } from '@tanstack/react-query';

export function useAppVersion() {
  return useQuery({
    queryKey: ['settings', 'about', 'appVersion'],
    queryFn: async () => {
      if (!window.mandoAPI) return '';
      const info = await window.mandoAPI.appInfo();
      return info.appVersion;
    },
  });
}
