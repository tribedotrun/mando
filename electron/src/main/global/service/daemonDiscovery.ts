import fs from 'fs';
import path from 'path';
import { getAppMode, getDataDir } from '#main/global/config/lifecycle';

interface DiscoveryCache {
  port: string | null;
  token: string | null;
}

const discovery: DiscoveryCache = {
  port: null,
  token: null,
};

function portFilePath(): string {
  const mode = getAppMode();
  const portFileName = mode === 'dev' ? 'daemon-dev.port' : 'daemon.port';
  return path.join(getDataDir(), portFileName);
}

function tokenFilePath(): string {
  return path.join(getDataDir(), 'auth-token');
}

export async function readPort() {
  if (discovery.port) return discovery.port;
  const content = await fs.promises.readFile(portFilePath(), 'utf-8');
  discovery.port = content.trim();
  return discovery.port;
}

export function readPortSync(): string {
  if (discovery.port) return discovery.port;
  const content = fs.readFileSync(portFilePath(), 'utf-8');
  discovery.port = content.trim();
  return discovery.port;
}

export async function readToken() {
  if (discovery.token) return discovery.token;
  const content = await fs.promises.readFile(tokenFilePath(), 'utf-8');
  discovery.token = content.trim();
  return discovery.token;
}

export function readTokenSync(): string {
  if (discovery.token) return discovery.token;
  const content = fs.readFileSync(tokenFilePath(), 'utf-8');
  discovery.token = content.trim();
  return discovery.token;
}

export function invalidateDiscoveryCache(): void {
  discovery.port = null;
  discovery.token = null;
}

export async function hasExternalGatewayToken(dataDir: string) {
  const envToken = process.env.MANDO_AUTH_TOKEN?.trim();
  if (envToken) return true;

  try {
    const content = await fs.promises.readFile(path.join(dataDir, 'auth-token'), 'utf-8');
    return content.trim().length > 0;
  } catch {
    return false;
  }
}
