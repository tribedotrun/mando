/**
 * Voice floating window — frameless, always-on-top pill.
 *
 * Toggle-to-talk:
 *   ⌥Space → pill appears, recording starts.
 *   ⌥Space again → stop recording, transcribe, submit.
 *   Audio plays → pill auto-hides.
 */
import { BrowserWindow, ipcMain, screen } from 'electron';

let voiceWindow: BrowserWindow | null = null;
let isRecording = false;
let windowReady = false;

export function createVoiceWindow(resolvePreload: () => string, rendererUrl: string): void {
  const display = screen.getPrimaryDisplay();
  const { width: screenW } = display.workAreaSize;
  const winW = 300;
  const winH = 50;
  const x = Math.round((screenW - winW) / 2);
  const y = 80;

  windowReady = false;

  voiceWindow = new BrowserWindow({
    width: winW,
    height: winH,
    x,
    y,
    alwaysOnTop: true,
    frame: false,
    show: false,
    resizable: false,
    skipTaskbar: true,
    transparent: true,
    hasShadow: false,
    focusable: true,
    webPreferences: {
      preload: resolvePreload(),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: false,
      webSecurity: true,
    },
  });

  voiceWindow.loadURL(rendererUrl);

  voiceWindow.webContents.on('did-finish-load', () => {
    windowReady = true;
  });

  voiceWindow.on('closed', () => {
    voiceWindow = null;
    isRecording = false;
    windowReady = false;
  });
}

export function onVoiceHotkeyDown(resolvePreload: () => string, rendererUrl: string): void {
  if (!voiceWindow) {
    createVoiceWindow(resolvePreload, rendererUrl);
    // Window just created — wait for load, then start recording
    voiceWindow!.webContents.on('did-finish-load', () => {
      isRecording = true;
      voiceWindow?.show();
      voiceWindow?.webContents.send('voice-start-recording');
    });
    return;
  }

  if (!voiceWindow.isVisible()) {
    // Window exists but hidden — show + start recording
    isRecording = true;
    voiceWindow.show();
    if (windowReady) {
      voiceWindow.webContents.send('voice-start-recording');
    }
  } else if (isRecording) {
    // Second press: stop recording → transcribe → submit
    isRecording = false;
    voiceWindow.webContents.send('voice-stop-recording');
  }
}

ipcMain.on('hide-voice-window', () => {
  voiceWindow?.hide();
  isRecording = false;
});
