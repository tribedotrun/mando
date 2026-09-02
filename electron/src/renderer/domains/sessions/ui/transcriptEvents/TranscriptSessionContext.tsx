import React, { createContext, useContext } from 'react';

const TranscriptSessionContext = createContext<string | null>(null);

interface TranscriptSessionProviderProps {
  children: React.ReactNode;
  sessionId: string;
}

export function TranscriptSessionProvider({
  children,
  sessionId,
}: TranscriptSessionProviderProps): React.ReactElement {
  return (
    <TranscriptSessionContext.Provider value={sessionId}>
      {children}
    </TranscriptSessionContext.Provider>
  );
}

export function useTranscriptSessionId(): string | null {
  return useContext(TranscriptSessionContext);
}
