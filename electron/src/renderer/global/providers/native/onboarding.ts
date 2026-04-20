export type ClaudeCodeCheckResult = {
  installed: boolean;
  version: string | null;
  works: boolean;
};

export function checkClaudeCode() {
  return window.mandoAPI.checkClaudeCode();
}

export function validateTelegramToken(token: string) {
  return window.mandoAPI.validateTelegramToken(token);
}

export function saveConfigLocal(config: string) {
  return window.mandoAPI.saveConfigLocal(config);
}

export function setupComplete(config: string) {
  return window.mandoAPI.setupComplete(config);
}

export function subscribeSetupProgress(callback: (step: string) => void): () => void {
  return window.mandoAPI.onSetupProgress(callback);
}
