import { invoke, convertFileSrc } from '@tauri-apps/api/core';
import type { ImageHostPlugin } from '../types/imageHostPlugin';
import { error } from '@tauri-apps/plugin-log';

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

const pluginCache = new Map<string, Promise<LoadedPlugin>>();
let entriesPromise: Promise<PluginEntry[]> | null = null;

async function queryPluginEntries(): Promise<PluginEntry[]> {
  const payload = await invoke<PluginEntry[]>('list_image_host_plugins');
  return payload.map((entry) => ({ ...entry }));
}

export async function getPluginEntries(force = false): Promise<PluginEntry[]> {
  if (force || !entriesPromise) {
    entriesPromise = queryPluginEntries().catch((err) => {
      entriesPromise = null;
      throw err;
    });
  }
  return entriesPromise;
}

/**
 * 解析插件脚本加载地址。
 * DEV 下通过 HTTP URL；PROD 下通过 convertFileSrc 转换为 tauri.localhost URL。
 */
function resolvePluginUrl(entry: PluginEntry): string {
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
    // 生产模式：使用 convertFileSrc 将资源路径转为可加载的 URL
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
