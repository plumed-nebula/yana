import type { ImageHostPlugin } from '../types/imageHostPlugin';

export interface PluginEntry {
  /** 插件唯一标识，用于存储和导航 */
  id: string;
  /** 插件脚本路径，基于应用根路径 */
  script: string;
}

export interface LoadedPlugin extends ImageHostPlugin {
  /** 注册表定义的唯一标识 */
  id: string;
  /** 运行时脚本的实际访问地址 */
  sourceUrl: string;
}

/**
 * 当前内置的所有插件入口。未来可以考虑通过后端读取目录生成。
 */
export const PLUGIN_ENTRIES: PluginEntry[] = [
  { id: 'smms', script: 'plugins/smms.js' },
];

function resolvePluginUrl(entry: PluginEntry): string {
  const base = window.location.origin;
  const normalized = entry.script.startsWith('/')
    ? entry.script
    : `/${entry.script}`;
  return new URL(normalized, base).href;
}

export async function loadPlugin(entry: PluginEntry): Promise<LoadedPlugin> {
  const url = resolvePluginUrl(entry);
  const mod = await import(/* @vite-ignore */ url);
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
  };
}
