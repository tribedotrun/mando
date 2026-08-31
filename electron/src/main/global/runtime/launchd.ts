/**
 * launchd integration — install/manage daemon plist and CLI binary.
 * App login item is managed by Electron's native `app.setLoginItemSettings` API.
 */
export { stageDaemonBinary } from '#main/global/runtime/launchdPaths';
export { installDaemonPlist, kickstartDaemon } from '#main/global/runtime/launchdServices';
export {
  installCliAndPlists,
  rollbackDaemonBinary,
  updateDaemonBinary,
} from '#main/global/runtime/launchdInstall';
