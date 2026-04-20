export type UpdateChannel = 'stable' | 'beta';

export function subscribeUpdateReady(
  callback: (info: { version: string; notes: string }) => void,
): () => void {
  return window.mandoAPI.updates.onUpdateReady(callback);
}

export function subscribeUpdateChecking(callback: () => void): () => void {
  return window.mandoAPI.updates.onUpdateChecking(callback);
}

export function subscribeUpdateNoUpdate(callback: () => void): () => void {
  return window.mandoAPI.updates.onUpdateNoUpdate(callback);
}

export function subscribeUpdateCheckError(callback: () => void): () => void {
  return window.mandoAPI.updates.onUpdateCheckError(callback);
}

export function subscribeUpdateCheckDone(callback: (info: { found: boolean }) => void): () => void {
  return window.mandoAPI.updates.onUpdateCheckDone(callback);
}

export function installUpdate() {
  return window.mandoAPI.updates.installUpdate();
}

export function checkForUpdates() {
  return window.mandoAPI.updates.checkForUpdates();
}

export function getPendingUpdate() {
  return window.mandoAPI.updates.getPending();
}

export function getUpdateAppVersion() {
  return window.mandoAPI.updates.appVersion();
}

export function getUpdateChannel() {
  return window.mandoAPI.updates.getChannel();
}

export function setUpdateChannel(channel: UpdateChannel) {
  return window.mandoAPI.updates.setChannel(channel);
}
