export function resolveLocalPath(input: string, cwd: string) {
  return window.mandoAPI.resolveLocalPath(input, cwd);
}

export function openLocalPath(path: string) {
  return window.mandoAPI.openLocalPath(path);
}
