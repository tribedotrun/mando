import React, { useState, useRef, useCallback, type MutableRefObject } from 'react';
import { useMountEffect } from '#renderer/hooks/useMountEffect';
import { initBaseUrl, postVoice, transcribeAudio } from '#renderer/api';
import { getErrorMessage } from '#renderer/utils';
import log from '#renderer/logger';

type VoiceState = 'idle' | 'listening' | 'transcribing' | 'thinking' | 'speaking' | 'error';

interface StateConfig {
  dotColor: string;
  label: string;
}

const STATE_DISPLAY: Record<VoiceState, StateConfig> = {
  idle: { dotColor: '#71717A', label: '' },
  listening: { dotColor: '#EF4444', label: 'Listening...' },
  transcribing: { dotColor: '#F59E0B', label: 'Transcribing...' },
  thinking: { dotColor: '#F59E0B', label: 'Thinking...' },
  speaking: { dotColor: '#22C55E', label: 'Speaking...' },
  error: { dotColor: '#EF4444', label: 'Error' },
};

// Shared AudioContext — reused across all playAudioBase64 calls to avoid leaks.
let sharedAudioCtx: AudioContext | null = null;
function getAudioContext(): AudioContext {
  if (!sharedAudioCtx || sharedAudioCtx.state === 'closed') {
    sharedAudioCtx = new AudioContext();
  }
  return sharedAudioCtx;
}

function playAudioBase64(b64: string): Promise<void> {
  return new Promise((resolve, reject) => {
    const raw = atob(b64);
    const buf = new ArrayBuffer(raw.length);
    const view = new Uint8Array(buf);
    for (let i = 0; i < raw.length; i++) view[i] = raw.charCodeAt(i);
    const ctx = getAudioContext();
    ctx.decodeAudioData(buf).then((decoded) => {
      const source = ctx.createBufferSource();
      source.buffer = decoded;
      source.connect(ctx.destination);
      source.onended = () => resolve();
      source.start();
    }, reject);
  });
}

function parseSseStream(
  reader: ReadableStreamDefaultReader<Uint8Array>,
  onEvent: (event: string, data: string) => void,
): Promise<void> {
  const decoder = new TextDecoder();
  let buffer = '';

  const processChunk = async (): Promise<void> => {
    const { done, value } = await reader.read();
    if (done) return;

    buffer += decoder.decode(value, { stream: true });
    const parts = buffer.split('\n\n');
    buffer = parts.pop() ?? '';

    for (const part of parts) {
      let eventType = 'message';
      let data = '';
      for (const line of part.split('\n')) {
        if (line.startsWith('event:')) eventType = line.slice(6).trim();
        else if (line.startsWith('data:')) data = line.slice(5).trim();
      }
      if (data) onEvent(eventType, data);
    }

    return processChunk();
  };

  return processChunk();
}

export function VoiceInput(): React.ReactElement {
  const [voiceState, setVoiceState] = useState<VoiceState>('idle');
  const [errorMsg, setErrorMsg] = useState('');
  const [sessionId, setSessionId] = useState<string | undefined>(() => {
    return localStorage.getItem('voice-session-id') ?? undefined;
  });
  const [initialized, setInitialized] = useState(false);

  const mediaRecorderRef = useRef<MediaRecorder | null>(null);
  const chunksRef = useRef<Blob[]>([]);
  const startRef: MutableRefObject<() => void> = useRef(() => {});
  const stopRef: MutableRefObject<() => void> = useRef(() => {});

  const hideWindow = useCallback(() => {
    setVoiceState('idle');
    window.mandoAPI?.hideVoiceWindow();
  }, []);

  const handleError = useCallback(
    (msg: string) => {
      setVoiceState('error');
      setErrorMsg(msg);
      setTimeout(hideWindow, 2000);
    },
    [hideWindow],
  );

  const submitTranscription = useCallback(
    async (text: string) => {
      // Strip "Mando, " prefix
      const cleaned = text.replace(/^mando,?\s*/i, '');
      if (!cleaned) {
        hideWindow();
        return;
      }

      setVoiceState('thinking');

      try {
        const res = await postVoice(cleaned, sessionId);
        if (!res.ok) {
          const err = await res.json().catch(() => ({ error: res.statusText }));
          handleError(`Error: ${err.error}`);
          return;
        }

        if (!res.body) {
          handleError('No response body from voice API');
          return;
        }
        const reader = res.body.getReader();
        let streamDone = false;
        let lastAudioPromise: Promise<void> | null = null;
        let textResponse = '';

        await parseSseStream(reader, (event, data) => {
          const parsed = JSON.parse(data);
          switch (event) {
            case 'text':
              if (parsed.chunk) textResponse = parsed.chunk;
              break;
            case 'audio':
              if (parsed.bytes) {
                setVoiceState('speaking');
                lastAudioPromise = playAudioBase64(parsed.bytes).then(() => {
                  // Only hide when stream is done AND this was the last audio.
                  if (streamDone) hideWindow();
                });
              }
              break;
            case 'error':
              if (parsed.source === 'tts' && textResponse) {
                // TTS failed — show the text response briefly instead.
                setVoiceState('speaking');
                setErrorMsg(textResponse);
                setTimeout(hideWindow, 3000);
              } else {
                handleError(`${parsed.source}: ${parsed.message}`);
              }
              break;
            case 'done':
              if (parsed.session_id) {
                setSessionId(parsed.session_id);
                localStorage.setItem('voice-session-id', parsed.session_id);
              }
              streamDone = true;
              // If no audio ever played, hide now.
              if (!lastAudioPromise) hideWindow();
              break;
          }
        });

        // Fallback: if stream ended without done/audio, hide
        if (!lastAudioPromise) hideWindow();
      } catch (err) {
        handleError(getErrorMessage(err, 'Request failed'));
      }
    },
    [sessionId, hideWindow, handleError],
  );

  const processRecording = useCallback(
    async (blob: Blob) => {
      if (blob.size === 0) {
        hideWindow();
        return;
      }

      setVoiceState('transcribing');

      try {
        const result = await transcribeAudio(blob);
        if (!result.text) {
          hideWindow();
          return;
        }
        await submitTranscription(result.text);
      } catch (err) {
        handleError(getErrorMessage(err, 'Transcription failed'));
      }
    },
    [submitTranscription, hideWindow, handleError],
  );

  const startRecording = useCallback(async () => {
    try {
      const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
      const recorder = new MediaRecorder(stream, { mimeType: 'audio/webm;codecs=opus' });
      mediaRecorderRef.current = recorder;
      chunksRef.current = [];

      recorder.ondataavailable = (e) => {
        if (e.data.size > 0) chunksRef.current.push(e.data);
      };

      recorder.onstop = () => {
        stream.getTracks().forEach((t) => t.stop());
        const blob = new Blob(chunksRef.current, { type: 'audio/webm' });
        processRecording(blob);
      };

      recorder.start();
      setVoiceState('listening');
    } catch (err) {
      handleError(getErrorMessage(err, 'Mic access failed'));
    }
  }, [processRecording, handleError]);

  const stopRecording = useCallback(() => {
    mediaRecorderRef.current?.stop();
  }, []);

  // Keep refs current so mount-time IPC listeners always call the latest version
  startRef.current = startRecording;
  stopRef.current = stopRecording;

  // Initialize base URL on mount
  useMountEffect(() => {
    initBaseUrl()
      .then(() => setInitialized(true))
      .catch((err) => {
        log.error('[VoiceInput] init failed:', err);
        setInitialized(true);
        setVoiceState('error');
        setErrorMsg('Voice unavailable');
      });
  });

  // IPC: main process tells us to start/stop recording via hotkey
  useMountEffect(() => {
    const api = window.mandoAPI;
    if (!api?.onVoiceStartRecording) return;
    api.onVoiceStartRecording(() => startRef.current());
    api.onVoiceStopRecording?.(() => stopRef.current());
    return () => {
      api.removeVoiceListeners?.();
    };
  });

  // Escape key hides window
  useMountEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        if (mediaRecorderRef.current?.state === 'recording') {
          mediaRecorderRef.current.stop();
        }
        hideWindow();
      }
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  });

  if (!initialized) {
    return <div style={{ width: 300, height: 50, background: 'transparent' }} />;
  }

  const { dotColor, label } = STATE_DISPLAY[voiceState];
  const displayLabel =
    voiceState === 'error' ? errorMsg : voiceState === 'speaking' && errorMsg ? errorMsg : label;

  if (voiceState === 'idle') {
    return <div style={{ width: 300, height: 50, background: 'transparent' }} />;
  }

  return (
    <div
      style={{
        width: 300,
        height: 50,
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        gap: 10,
        background: 'rgba(28, 28, 30, 0.95)',
        borderRadius: 25,
        fontFamily: 'Geist, system-ui, sans-serif',
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
        ...({ WebkitAppRegion: 'drag' } as any),
      }}
    >
      <span
        style={{
          width: 10,
          height: 10,
          borderRadius: '50%',
          backgroundColor: dotColor,
          flexShrink: 0,
          animation: voiceState === 'listening' ? 'pulse 1.5s ease-in-out infinite' : undefined,
        }}
      />
      <span
        style={{
          color: '#A1A1AA',
          fontSize: 13,
          fontWeight: 500,
          letterSpacing: '0.02em',
          whiteSpace: 'nowrap',
          overflow: 'hidden',
          textOverflow: 'ellipsis',
          maxWidth: 230,
        }}
      >
        {displayLabel}
      </span>
      <style>{`
        @keyframes pulse {
          0%, 100% { opacity: 1; transform: scale(1); }
          50% { opacity: 0.5; transform: scale(0.85); }
        }
      `}</style>
    </div>
  );
}
