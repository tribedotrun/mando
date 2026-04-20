import { app } from 'electron';
import path from 'path';
import { mkdirSync, readFileSync, writeFileSync } from 'fs';
import { channelConfigSchema, type UpdateChannel } from '#main/updater/types/updater';
import { UPDATE_SERVER, getChannelConfigPath } from '#main/updater/config/updater';
import { errCode } from '#main/updater/service/updater';
import { parseJsonTextWith } from '#result';

export function readChannel(): UpdateChannel {
  try {
    const raw = readFileSync(getChannelConfigPath(), 'utf-8');
    const parsed = parseJsonTextWith(raw, channelConfigSchema, 'file:update-channel-config');
    if (parsed.isOk() && parsed.value.channel === 'beta') return 'beta';
  } catch (err) {
    if (errCode(err) !== 'ENOENT') {
      return 'stable';
    }
  }
  return 'stable';
}

export function writeChannel(channel: UpdateChannel): void {
  const configPath = getChannelConfigPath();
  mkdirSync(path.dirname(configPath), { recursive: true });
  writeFileSync(configPath, JSON.stringify({ channel }), 'utf-8');
}

export function resolveUpdateServer(): string {
  return process.env.MANDO_UPDATE_SERVER || UPDATE_SERVER;
}

export function buildFeedUrl(): string {
  const channel = readChannel();
  const channelParam = channel !== 'stable' ? `?channel=${channel}` : '';
  return `${resolveUpdateServer()}/update/darwin/${process.arch}/${app.getVersion()}${channelParam}`;
}
