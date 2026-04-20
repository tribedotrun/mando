export function getGatewayUrl() {
  return window.mandoAPI.gatewayUrl();
}

export function getAppMode() {
  return window.mandoAPI.appMode();
}

export function getDevGitInfo() {
  return window.mandoAPI.devGitInfo();
}

export function getAppInfo() {
  return window.mandoAPI.appInfo();
}

export function hasConfig() {
  return window.mandoAPI.hasConfig();
}

export function readConfigFallback() {
  return window.mandoAPI.readConfig();
}

export function restartDaemon() {
  return window.mandoAPI.restartDaemon();
}

export function setLoginItem(enabled: boolean) {
  return window.mandoAPI.setLoginItem(enabled);
}

export function subscribeShortcut(callback: (action: string) => void): () => void {
  return window.mandoAPI.onShortcut(callback);
}
