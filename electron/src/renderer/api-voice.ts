import { buildUrl } from '#renderer/api';
import log from '#renderer/logger';

export async function postVoice(text: string, sessionId?: string): Promise<Response> {
  const res = await fetch(buildUrl('/api/voice'), {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ text, session_id: sessionId }),
  });
  if (!res.ok) {
    log.warn(`[voice] POST /api/voice failed with status ${res.status}`);
  }
  return res;
}

export async function transcribeAudio(audioBlob: Blob): Promise<{ text: string }> {
  const formData = new FormData();
  formData.append('file', audioBlob, 'audio.webm');
  const res = await fetch(buildUrl('/api/voice/transcribe'), {
    method: 'POST',
    body: formData,
  });
  if (!res.ok) {
    const errBody = await res.json().catch(() => ({ error: res.statusText }));
    throw new Error(errBody.error || `HTTP ${res.status}`);
  }
  return res.json();
}
