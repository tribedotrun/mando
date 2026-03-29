import { buildUrl, buildAuthHeaders } from '#renderer/api';

export async function postVoice(text: string, sessionId?: string): Promise<Response> {
  return fetch(buildUrl('/api/voice'), {
    method: 'POST',
    headers: buildAuthHeaders({ 'Content-Type': 'application/json' }),
    body: JSON.stringify({ text, session_id: sessionId }),
  });
}

export async function transcribeAudio(audioBlob: Blob): Promise<{ text: string }> {
  const formData = new FormData();
  formData.append('file', audioBlob, 'audio.webm');
  const res = await fetch(buildUrl('/api/voice/transcribe'), {
    method: 'POST',
    headers: buildAuthHeaders(),
    body: formData,
  });
  if (!res.ok) {
    const errBody = await res.json().catch(() => ({ error: res.statusText }));
    throw new Error(errBody.error || `HTTP ${res.status}`);
  }
  return res.json();
}
