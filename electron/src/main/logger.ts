import log from 'electron-log/main';
import os from 'os';
import path from 'path';
import fs from 'fs';

// Write to MANDO_LOG_DIR (dev/sandbox) or ~/.mando/logs/ (production).
// Daily rotation matching the Rust daemon/TG bot pattern: electron.jsonl.YYYY-MM-DD
const dataDir = process.env.MANDO_DATA_DIR || path.join(os.homedir(), '.mando');
const logDir = process.env.MANDO_LOG_DIR || path.join(dataDir, 'logs');
fs.mkdirSync(logDir, { recursive: true });

function todaySuffix(): string {
  return new Date().toISOString().slice(0, 10); // YYYY-MM-DD
}

let currentDate = todaySuffix();
let currentLogPath = path.join(logDir, `electron.jsonl.${currentDate}`);

log.transports.file.fileName = `electron.jsonl.${currentDate}`;
log.transports.file.resolvePathFn = () => {
  const today = todaySuffix();
  if (today !== currentDate) {
    currentDate = today;
    currentLogPath = path.join(logDir, `electron.jsonl.${today}`);
  }
  return currentLogPath;
};
log.transports.file.format = ({ data, level, message: msg }) => {
  const text = data.map((d: unknown) => (typeof d === 'string' ? d : JSON.stringify(d))).join(' ');
  return [JSON.stringify({ timestamp: msg.date.toISOString(), level, message: text })];
};
log.transports.file.maxSize = 10 * 1024 * 1024; // 10MB per file
log.transports.file.archiveLogFn = (oldLogFile) => {
  try {
    fs.unlinkSync(oldLogFile.path);
  } catch (e) {
    console.error('[logger] failed to remove rotated log:', oldLogFile.path, e);
  }
};

// Console transport: keep it readable
log.transports.console.format = '[{level}] {text}';

export default log;
