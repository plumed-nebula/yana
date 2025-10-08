import { defineConfig } from 'vite';
import vue from '@vitejs/plugin-vue';
import {
  access,
  copyFile,
  mkdir,
  readdir,
  readFile,
  rm,
} from 'node:fs/promises';
import { constants as fsConstants } from 'node:fs';
import { dirname, extname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const host = process.env.TAURI_DEV_HOST;

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const pluginSourceDir = resolve(__dirname, 'src', 'plugins');
const pluginOutputDir = resolve(__dirname, 'dist', 'plugins');

async function pathExists(path: string): Promise<boolean> {
  try {
    await access(path, fsConstants.F_OK);
    return true;
  } catch {
    return false;
  }
}

async function copyDirectory(src: string, dest: string): Promise<void> {
  await mkdir(dest, { recursive: true });
  const entries = await readdir(src, { withFileTypes: true });
  for (const entry of entries) {
    const srcPath = resolve(src, entry.name);
    const destPath = resolve(dest, entry.name);
    if (entry.isDirectory()) {
      await copyDirectory(srcPath, destPath);
    } else if (entry.isFile()) {
      await copyFile(srcPath, destPath);
    }
  }
}

function preservePluginScripts() {
  return {
    name: 'preserve-plugin-scripts',
    apply: 'build' as const,
    async closeBundle() {
      if (!(await pathExists(pluginSourceDir))) {
        return;
      }

      await rm(pluginOutputDir, { recursive: true, force: true });
      await copyDirectory(pluginSourceDir, pluginOutputDir);
    },
  };
}

const MIME_MAP: Record<string, string> = {
  '.js': 'application/javascript',
  '.mjs': 'application/javascript',
  '.cjs': 'application/javascript',
  '.json': 'application/json',
  '.map': 'application/json',
};

function detectMimeType(filePath: string): string {
  const ext = extname(filePath).toLowerCase();
  return MIME_MAP[ext] ?? 'text/plain; charset=utf-8';
}

function servePluginScripts() {
  return {
    name: 'serve-plugin-scripts',
    apply: 'serve' as const,
    configureServer(server: import('vite').ViteDevServer) {
      server.middlewares.use(async (req, res, next) => {
        const url = req.url;
        if (!url || !url.startsWith('/plugins/')) {
          return next();
        }

        const relativePath = url.replace(/^\/plugins\//, '');
        const requestedPath = resolve(pluginSourceDir, relativePath);

        if (!requestedPath.startsWith(pluginSourceDir)) {
          res.statusCode = 403;
          res.end('Forbidden');
          return;
        }

        if (!(await pathExists(requestedPath))) {
          return next();
        }

        try {
          const body = await readFile(requestedPath);
          res.statusCode = 200;
          res.setHeader('Content-Type', detectMimeType(requestedPath));
          res.end(body);
        } catch (error) {
          server.config.logger.error(
            `[plugins] Failed to serve ${requestedPath}: ${String(error)}`
          );
          res.statusCode = 500;
          res.end('Internal Server Error');
        }
      });

      server.watcher.add(pluginSourceDir);
    },
  };
}

// https://vite.dev/config/
export default defineConfig(async () => ({
  plugins: [vue(), servePluginScripts(), preservePluginScripts()],

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: 'ws',
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ['**/src-tauri/**'],
    },
  },
}));
