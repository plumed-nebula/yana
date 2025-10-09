import { invoke } from '@tauri-apps/api/core';
import { fetch, type ClientOptions } from '@tauri-apps/plugin-http';
import { debug, error, info, warn } from '@tauri-apps/plugin-log';

/**
 * 插件参数类型枚举，用于提示界面如何渲染输入控件。
 */
export type PluginParameterType =
  | 'text'
  | 'password'
  | 'number'
  | 'boolean'
  | 'select'
  | 'textarea';

export interface PluginParameterOption<T = string> {
  label: string;
  value: T;
}

export interface PluginParameterDescriptor<T = unknown> {
  /** 用作存储和提交的键名 */
  key: string;
  /** 展示给用户的名称 */
  label: string;
  /** 控件类型 */
  type: PluginParameterType;
  /** 是否必填 */
  required?: boolean;
  /** 默认值 */
  defaultValue?: T;
  /** 用于 select 等控件的枚举值 */
  options?: PluginParameterOption<T>[];
  /** 额外描述或提示 */
  description?: string;
}

/**
 * 插件声明自身支持的文件类型。
 * - 当 both mimeTypes 与 extensions 都为空时视为“接受所有类型”。
 * - extensions 不需要带点，例如 "png"、"jpg"。
 */
export interface PluginSupportedFileType {
  /** MIME 类型列表，例如 image/png */
  mimeTypes?: string[];
  /** 文件扩展名列表，不区分大小写 */
  extensions?: string[];
  /** 友好的说明文字，用于文件选择器提示 */
  description?: string;
}

export interface PluginUploadResult {
  /** 上传后返回的可访问链接 */
  url: string;
  /** 删除时使用的标识，由插件自行决定 */
  deleteId: string;
  /** 可选的附加信息 */
  metadata?: Record<string, unknown>;
}

export interface PluginDeleteResult {
  success: boolean;
  message?: string;
}

export interface BackendUploadResponse {
  status: number;
  headers: Array<[string, string]>;
  body: unknown;
  rawText: string;
}

export type BackendUploadFormat = 'binary' | 'form' | 'base64';

export interface BackendUploadConfig {
  url: string;
  headers?: Record<string, string>;
  fieldName?: string;
  additionalFields?: Record<string, string>;
  jsonKey?: string;
  additionalJson?: Record<string, unknown>;
  fileName?: string;
  contentType?: string;
  timeoutMs?: number;
}

export interface BackendUploadOptions {
  filePath: string;
  format: BackendUploadFormat;
  config: BackendUploadConfig;
}

export interface PluginRuntimeContext {
  /**
   * 对后端 `upload_image` 命令的安全再封装，提供统一上传能力。
   */
  uploadViaBackend: (
    options: BackendUploadOptions
  ) => Promise<BackendUploadResponse>;
  /**
   * 统一日志接口，底层使用 tauri-plugin-log。
   */
  logger: {
    debug: typeof debug;
    info: typeof info;
    warn: typeof warn;
    error: typeof error;
  };
  /**
   * 基于 @tauri-apps/api/http 的网络请求能力，由后端驱动。
   */
  httpRequest: (
    input: RequestInfo | URL,
    init?: RequestInit & ClientOptions
  ) => Promise<Response>;
}

export type PluginUploadFunction = (
  filePath: string,
  originalFileName: string,
  params: Record<string, unknown>,
  context: PluginRuntimeContext
) => Promise<PluginUploadResult>;

export type PluginDeleteFunction = (
  deleteId: string,
  context: PluginRuntimeContext
) => Promise<PluginDeleteResult>;

export interface ImageHostPlugin {
  /** 插件名称 */
  name: string;
  /** 插件作者，可选 */
  author?: string;
  /** 插件版本，可选 */
  version?: string;
  /**
   * 插件支持的文件类型声明。
   * 空数组或未提供时表示不做限制。
   */
  supportedFileTypes?: PluginSupportedFileType[];
  /** 用户在界面上需要配置的参数 */
  parameters: PluginParameterDescriptor[];
  /** 上传逻辑 */
  upload: PluginUploadFunction;
  /** 删除逻辑 */
  remove: PluginDeleteFunction;
  /** 可选的描述信息 */
  description?: string;
}

export const createPluginRuntimeContext = (): PluginRuntimeContext => ({
  uploadViaBackend: async ({ filePath, format, config }) => {
    const payload = {
      filePath,
      format,
      config: {
        url: config.url,
        headers: config.headers ?? {},
        fieldName: config.fieldName,
        additionalFields: config.additionalFields ?? {},
        jsonKey: config.jsonKey,
        additionalJson: config.additionalJson ?? {},
        fileName: config.fileName,
        contentType: config.contentType,
        timeoutMs: config.timeoutMs,
      },
    };

    return invoke<BackendUploadResponse>('upload_image', payload);
  },
  logger: {
    debug,
    info,
    warn,
    error,
  },
  httpRequest: (input, init) => fetch(input, init),
});
