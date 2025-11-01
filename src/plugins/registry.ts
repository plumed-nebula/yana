import { invoke, convertFileSrc } from '@tauri-apps/api/core';
import type { ImageHostPlugin } from '../types/imageHostPlugin';
import { error, debug } from '@tauri-apps/plugin-log';

export interface PluginEntry {
  /** 插件唯一标识，用于存储和导航 */
  id: string;
  /** 插件脚本路径，绝对路径 */
  script: string;
}

export interface LoadedPlugin extends ImageHostPlugin {
  /** 注册表定义的唯一标识 */
  id: string;
  /** 运行时脚本的实际访问地址 */
  sourceUrl: string;
}

// 使用 Vite 的 import.meta.glob 静态导入所有插件
// 这样在 Android 构建时，插件会被打包进主 bundle，无需动态加载
// 不指定 import，这样可以获取完整的模块对象（支持 default 和命名导出）
const builtinPluginModules = import.meta.glob<
  ImageHostPlugin | { default?: ImageHostPlugin }
>('./*.js', {
  eager: false,
});

// 启动时输出已发现的内置插件
try {
  const moduleKeys = Object.keys(builtinPluginModules);
  debug(
    `[registry] Discovered ${
      moduleKeys.length
    } builtin plugin modules: ${moduleKeys.join(', ')}`
  );
} catch (e) {
  /* ignore */
}

const pluginCache = new Map<string, Promise<LoadedPlugin>>();
let entriesPromise: Promise<PluginEntry[]> | null = null;
// cache last loaded entries so callers can check readiness even if the event was
// dispatched before they registered a listener.
let lastEntries: PluginEntry[] | null = null;
let entriesLoaded = false;

async function queryPluginEntries(): Promise<PluginEntry[]> {
  const payload = await invoke<PluginEntry[]>('list_image_host_plugins');
  return payload.map((entry) => ({ ...entry }));
}

export async function getPluginEntries(force = false): Promise<PluginEntry[]> {
  try {
    debug('[registry] 开始获取插件条目');
  } catch (e) {
    /* ignore */
  }
  if (force || !entriesPromise) {
    entriesLoaded = false;
    entriesPromise = queryPluginEntries()
      .then((entries) => {
        try {
          debug('[registry] 插件条目加载完成，派发 imageHosts:ready 事件');
        } catch (e) {
          /* ignore */
        }
        // cache entries and mark loaded before dispatching event
        lastEntries = entries;
        entriesLoaded = true;
        // notify listeners that plugin entries have been loaded
        try {
          window.dispatchEvent(new CustomEvent('imageHosts:ready'));
        } catch (e) {
          /* ignore */
        }
        return entries;
      })
      .catch((err) => {
        try {
          debug('[registry] 获取插件条目失败: ' + String(err));
        } catch (e) {
          /* ignore */
        }
        entriesPromise = null;
        entriesLoaded = true; // mark as loaded even on failure to avoid blocking callers
        lastEntries = null;
        throw err;
      });
  }
  return entriesPromise;
}

/**
 * 是否已经完成插件条目的初次加载（成功或失败）。
 * 供组件在注册事件监听后立即查询，处理“事件先发后听”的情况。
 */
export function arePluginEntriesLoaded(): boolean {
  return entriesLoaded;
}

/**
 * 返回最后一次成功加载的插件条目，若无则返回 null。
 */
export function getLoadedPluginEntries(): PluginEntry[] | null {
  return lastEntries;
}

/**
 * 解析插件脚本加载地址。
 * DEV 下通过 HTTP URL；PROD 下通过 convertFileSrc 转换为 tauri.localhost URL。
 */
function resolvePluginUrl(entry: PluginEntry): string {
  // 无论开发还是生产模式，都先检查是否是 asset:// 协议（Android）
  if (entry.script.startsWith('asset://')) {
    // Android 平台：后端已返回 asset:// URL，直接使用
    try {
      debug(
        `[registry] Using asset:// URL directly for ${entry.id}: ${entry.script}`
      );
    } catch (e) {
      /* ignore */
    }
    return entry.script;
  }

  if (import.meta.env.DEV) {
    // If the script looks like an absolute filesystem path (Windows drive letter or leading '/'),
    // use convertFileSrc so that the dev webview can load it via the tauri asset handler.
    const looksLikeAbsolutePath = /(^[A-Za-z]:\\)|(^\\\\\\?\\)|(^\/)/.test(
      entry.script
    );
    if (looksLikeAbsolutePath) {
      try {
        return convertFileSrc(entry.script);
      } catch (e) {
        // fallback to dev server URL if convertFileSrc fails for some reason
      }
    }
    const base = window.location.origin;
    const normalized = entry.script.startsWith('/')
      ? entry.script
      : `/${entry.script}`;
    return new URL(normalized, base).href;
  } else {
    // 桌面平台：使用 convertFileSrc 将文件系统路径转为可加载的 URL
    const url = convertFileSrc(entry.script);

    /*
    // 原先的 fetch 打印逻辑（调试用），已注释以减少启动噪音。若需要再次开启，取消注释即可。
    (async () => {
      try {
        const resp = await fetch(url);
        const text = await resp.text();
        info(
          `[imageHosts] resolvePluginUrl fetched plugin text for ${entry.id} ${url}`
        );
        info(text);
      } catch (e) {
        error(
          `[imageHosts] resolvePluginUrl failed to fetch plugin text for ${
            entry.id
          } ${url}: ${e instanceof Error ? e.message : String(e)}`
        );
      }
    })();
    */

    return url;
  }
}

export async function loadPlugin(entry: PluginEntry): Promise<LoadedPlugin> {
  if (!pluginCache.has(entry.id)) {
    const promise = (async () => {
      // 内置 S3 插件，调用后端 S3 命令，前端不暴露签名逻辑
      if (entry.id === 's3') {
        interface S3UploadBackendResult {
          url: string;
          deleteId: string;
          metadata?: Record<string, unknown>;
        }

        interface S3DeleteBackendResult {
          success: boolean;
          message?: string;
        }

        const coerceString = (value: unknown): string =>
          typeof value === 'string'
            ? value
            : value != null
            ? String(value)
            : '';

        const asOptionalString = (value: unknown): string | null => {
          if (typeof value !== 'string') return null;
          const trimmed = value.trim();
          return trimmed.length > 0 ? trimmed : null;
        };

        const asBoolean = (value: unknown, fallback = false): boolean => {
          if (typeof value === 'boolean') return value;
          if (typeof value === 'string') {
            const lowered = value.trim().toLowerCase();
            if (lowered === 'true') return true;
            if (lowered === 'false') return false;
          }
          if (typeof value === 'number') return value !== 0;
          return fallback;
        };

        const stub: LoadedPlugin = {
          id: 's3',
          sourceUrl: '',
          name: 'S3 上传',
          author: '官方内置',
          version: '1.0.0',
          description: '使用后端 S3 接口上传',
          supportedFileTypes: [
            {
              description: '常见图片类型',
              mimeTypes: ['image/png', 'image/jpeg', 'image/webp', 'image/gif'],
            },
          ],
          parameters: [
            { key: 'bucket', label: 'S3 Bucket', type: 'text', required: true },
            {
              key: 'region',
              label: '区域 (region)',
              type: 'text',
              required: true,
            },
            {
              key: 'accessKeyId',
              label: 'Access Key ID',
              type: 'text',
              required: true,
            },
            {
              key: 'secretAccessKey',
              label: 'Secret Access Key',
              type: 'password',
              required: true,
            },
            {
              key: 'endpoint',
              label: '自定义 Endpoint',
              type: 'text',
              description:
                'S3 兼容服务的根地址，例如 https://s3.example.com，留空则使用官方默认',
            },
            {
              key: 'forcePathStyle',
              label: '强制 Path-Style',
              type: 'boolean',
              defaultValue: false,
              description:
                '兼容部分只支持 Path Style 的对象存储 (MinIO、部分兼容服务)',
            },
            {
              key: 'objectPrefix',
              label: '对象前缀 (可选)',
              type: 'text',
              description:
                '可选，前缀会追加在自动生成的目录前，例如 uploads 或 projectA',
            },
            {
              key: 'acl',
              label: 'ACL 权限 (可选)',
              type: 'select',
              defaultValue: 'private',
              options: [
                { label: 'private', value: 'private' },
                { label: 'public-read', value: 'public-read' },
                { label: 'public-read-write', value: 'public-read-write' },
                { label: 'authenticated-read', value: 'authenticated-read' },
                { label: 'aws-exec-read', value: 'aws-exec-read' },
                { label: 'bucket-owner-read', value: 'bucket-owner-read' },
                {
                  label: 'bucket-owner-full-control',
                  value: 'bucket-owner-full-control',
                },
                { label: '不指定 (沿用存储桶默认)', value: '' },
              ],
            },
            {
              key: 'publicBaseUrl',
              label: '对外访问根地址 (可选)',
              type: 'text',
              description:
                '例如 https://cdn.example.com，返回链接会拼接该前缀，留空则使用 S3 默认域名',
            },
          ],
          upload: async (filePath, originalFileName, params, _context) => {
            // 调用后端命令执行 S3 上传
            const requireString = (value: unknown, label: string): string => {
              const coerced = coerceString(value).trim();
              if (!coerced) {
                throw new Error(`${label} 不能为空`);
              }
              return coerced;
            };

            const bucket = requireString(params.bucket, 'S3 Bucket');
            const region = requireString(params.region, '区域 (region)');
            const accessKeyId = requireString(
              params.accessKeyId,
              'Access Key ID'
            );
            const secretAccessKey = requireString(
              params.secretAccessKey,
              'Secret Access Key'
            );

            const result = await invoke<S3UploadBackendResult>('s3_upload', {
              filePath,
              originalFileName,
              bucket,
              region,
              accessKeyId,
              secretAccessKey,
              endpoint: asOptionalString(params.endpoint),
              forcePathStyle: asBoolean(params.forcePathStyle, false),
              objectPrefix: asOptionalString(params.objectPrefix),
              acl: asOptionalString(params.acl),
              publicBaseUrl: asOptionalString(params.publicBaseUrl),
            });
            return {
              url: result.url,
              deleteId: result.deleteId,
              metadata: result.metadata,
            };
          },
          remove: async (deleteId, _context) => {
            if (!deleteId) {
              return {
                success: false,
                message: '缺少 deleteId，无法执行 S3 删除。',
              };
            }

            const settings = await invoke<Record<string, unknown> | null>(
              'load_image_host_settings',
              { pluginId: 's3' }
            ).catch(() => null);

            const accessKeyId =
              asOptionalString(settings?.['accessKeyId']) ?? '';
            const secretAccessKey =
              asOptionalString(settings?.['secretAccessKey']) ?? '';

            if (!accessKeyId || !secretAccessKey) {
              return {
                success: false,
                message:
                  '请先在 S3 插件设置中填写 Access Key 和 Secret Access Key。',
              };
            }

            const result = await invoke<S3DeleteBackendResult>('s3_delete', {
              deleteId,
              accessKeyId,
              secretAccessKey,
            }).catch((error: unknown) => ({
              success: false,
              message:
                error instanceof Error
                  ? error.message
                  : String(error ?? '未知错误'),
            }));

            return result;
          },
        };
        return stub;
      }

      // 检查是否有静态导入的内置插件（用于 Android 等不支持动态加载的平台）
      const builtinModulePath = `./${entry.id}.js`;
      if (builtinPluginModules[builtinModulePath]) {
        try {
          debug(
            `[registry] Loading builtin plugin ${entry.id} from static imports`
          );
        } catch (e) {
          /* ignore */
        }

        // 使用静态导入的模块
        const moduleExports = await builtinPluginModules[builtinModulePath]();

        // 处理两种导出格式：export default 和 命名导出
        const pluginModule: ImageHostPlugin =
          'default' in moduleExports && moduleExports.default
            ? moduleExports.default
            : (moduleExports as ImageHostPlugin);

        if (!pluginModule || typeof pluginModule.upload !== 'function') {
          throw new Error(`插件 ${entry.id} 缺少上传函数实现`);
        }
        if (typeof pluginModule.remove !== 'function') {
          throw new Error(`插件 ${entry.id} 缺少删除函数实现`);
        }

        return {
          id: entry.id,
          sourceUrl: builtinModulePath,
          name: pluginModule.name ?? entry.id,
          author: pluginModule.author,
          version: pluginModule.version,
          description: pluginModule.description,
          supportedFileTypes: pluginModule.supportedFileTypes,
          parameters: pluginModule.parameters ?? [],
          upload: pluginModule.upload,
          remove: pluginModule.remove,
        } satisfies LoadedPlugin;
      }

      // 回退到动态加载（桌面平台或用户自定义插件）
      const url = resolvePluginUrl(entry);
      let mod: any;
      try {
        mod = await import(/* @vite-ignore */ url);
      } catch (importErr) {
        // 动态 import 失败，尝试降级：fetch 源码并使用 Blob URL 导入
        try {
          error(
            `[imageHosts] dynamic import failed for ${entry.id} ${url}: ${
              importErr instanceof Error ? importErr.message : String(importErr)
            }`
          );
        } catch {}
        try {
          const resp = await fetch(url);
          const text = await resp.text();
          const blob = new Blob([text], { type: 'application/javascript' });
          const blobUrl = URL.createObjectURL(blob);
          try {
            mod = await import(/* @vite-ignore */ blobUrl);
          } finally {
            // 立即释放 blob URL，模块已被加载到内存
            URL.revokeObjectURL(blobUrl);
          }
        } catch (fallbackErr) {
          throw new Error(
            `failed to load plugin ${entry.id}: ${
              fallbackErr instanceof Error
                ? fallbackErr.message
                : String(fallbackErr)
            }`
          );
        }
      }
      const pluginModule: ImageHostPlugin | undefined = (mod.default ??
        mod) as ImageHostPlugin;

      if (!pluginModule || typeof pluginModule.upload !== 'function') {
        throw new Error(`插件 ${entry.id} 缺少上传函数实现`);
      }
      if (typeof pluginModule.remove !== 'function') {
        throw new Error(`插件 ${entry.id} 缺少删除函数实现`);
      }

      return {
        id: entry.id,
        sourceUrl: url,
        name: pluginModule.name ?? entry.id,
        author: pluginModule.author,
        version: pluginModule.version,
        description: pluginModule.description,
        supportedFileTypes: pluginModule.supportedFileTypes,
        parameters: pluginModule.parameters ?? [],
        upload: pluginModule.upload,
        remove: pluginModule.remove,
      } satisfies LoadedPlugin;
    })();

    pluginCache.set(entry.id, promise);
  }

  return pluginCache.get(entry.id)!;
}

export function clearPluginCache() {
  pluginCache.clear();
  entriesPromise = null;
}
