/**
 * Local HTTP server for serving the renderer in production builds.
 * In dev, Vite's dev server is used instead.
 */
import path from 'path';
import fs from 'fs';
import http from 'http';
import log from '#main/global/providers/logger';

const MIME_TYPES: Record<string, string> = {
  '.html': 'text/html',
  '.js': 'application/javascript',
  '.css': 'text/css',
  '.json': 'application/json',
  '.png': 'image/png',
  '.svg': 'image/svg+xml',
  '.woff': 'font/woff',
  '.woff2': 'font/woff2',
};

export function startRendererServer(
  rendererDir: string,
): Promise<{ port: number; server: http.Server }> {
  return new Promise((resolve, reject) => {
    const server = http.createServer((req, res) => {
      const urlPath = req.url === '/' ? '/index.html' : req.url || '/index.html';
      const cleanPath = urlPath.split('?')[0];
      const filePath = path.join(rendererDir, cleanPath);
      if (!filePath.startsWith(rendererDir)) {
        res.writeHead(403);
        res.end('Forbidden');
        return;
      }
      const ext = path.extname(filePath);
      const contentType = MIME_TYPES[ext] || 'application/octet-stream';
      fs.readFile(filePath, (err, data) => {
        if (err) {
          const code = (err as NodeJS.ErrnoException).code;
          if (code === 'ENOENT') {
            res.writeHead(404);
            res.end('Not found');
            return;
          }
          log.error(
            `[renderer-server] readFile failed for ${cleanPath} (${code ?? 'unknown'}): ${err.message}`,
          );
          res.writeHead(500);
          res.end(`Internal error reading ${cleanPath}: ${err.message}`);
          return;
        }
        res.writeHead(200, { 'Content-Type': contentType });
        res.end(data);
      });
    });
    server.listen(0, '127.0.0.1', () => {
      const addr = server.address();
      if (addr && typeof addr !== 'string') {
        resolve({ port: addr.port, server });
      } else {
        reject(new Error('Failed to start renderer server'));
      }
    });
    server.on('error', reject);
  });
}
