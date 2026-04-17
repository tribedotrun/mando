import { QueryClient } from '@tanstack/react-query';

export const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 5_000,
      // Read queries retry up to 2 times with exponential backoff so a transient daemon hiccup doesn't surface as a hard error to the user.
      retry: 2,
      retryDelay: (attemptIndex) => Math.min(1000 * 2 ** attemptIndex, 10_000),
      refetchOnWindowFocus: false,
    },
    mutations: {
      // Mutations never retry automatically to avoid duplicating side effects.
      retry: 0,
    },
  },
});
