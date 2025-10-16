# 图床插件开发规范

本文档面向图床插件开发者，说明插件脚本中必须导出的接口、参数约定、函数签名、可用运行时 API 以及返回值规范。

---

## 一、插件接口（ImageHostPlugin）

插件必须导出一个符合以下接口声明的对象：

```ts
interface ImageHostPlugin {
  /** 插件名称 (必填) */
  name: string;
  /** 插件作者 (可选) */
  author?: string;
  /** 插件版本 (可选) */
  version?: string;
  /** 插件说明 (可选) */
  description?: string;

  /** 支持的文件类型列表 (可选) */
  supportedFileTypes?: PluginSupportedFileType[];

  /** 参数描述，用于 UI 渲染 */
  parameters: PluginParameterDescriptor[];

  /** 上传函数 */
  upload: PluginUploadFunction;

  /** 删除函数 */
  remove: PluginDeleteFunction;
}
```

### 2.1 参数描述 (`PluginParameterDescriptor`)

```ts
interface PluginParameterDescriptor<T = unknown> {
  key: string;               // 存储与提交的键名
  label: string;             // UI 显示名称
  type: 'text' | 'password' | 'number' | 'boolean' | 'select' | 'textarea';
  required?: boolean;        // 是否必填
  defaultValue?: T;          // 默认值
  options?: PluginParameterOption<T>[]; // select 枚举项
  description?: string;      // 额外提示
}

interface PluginParameterOption<T = string> {
  label: string;
  value: T;
}
```

### 2.2 文件类型支持声明 (`PluginSupportedFileType`)

```ts
interface PluginSupportedFileType {
  mimeTypes?: string[];   // MIME 列表
  extensions?: string[];  // 扩展名列表（不含点）
  description?: string;   // 友好说明
}
```

---

## 二、核心方法签名

```ts
/**
 * 上传函数
 * @param filePath 本地文件绝对路径
 * @param originalFileName 原始文件名
 * @param params      来自 UI 的参数键值对
 * @param context     运行时上下文
 * @returns 插件上传结果
 */
type PluginUploadFunction = (
  filePath: string,
  originalFileName: string,
  params: Record<string, unknown>,
  context: PluginRuntimeContext
) => Promise<PluginUploadResult>;

/**
 * 删除函数
 * @param deleteId 插件生成的删除标识
 * @param context  运行时上下文
 * @returns 删除执行结果
 */
type PluginDeleteFunction = (
  deleteId: string,
  context: PluginRuntimeContext
) => Promise<PluginDeleteResult>;
```

---

## 三、运行时能力 (`PluginRuntimeContext`)

从 `src/types/imageHostPlugin.ts` 创建：

```ts
interface PluginRuntimeContext {
  /**
   * 通过后端 `upload_image` 命令上传文件
   */
  uploadViaBackend: (options: BackendUploadOptions) => Promise<BackendUploadResponse>;

  /** 统一日志接口 (tauri-plugin-log) */
  logger: {
    debug: typeof debug;
    info: typeof info;
    warn: typeof warn;
    error: typeof error;
  };

  /** 基于 tauri-plugin-http 的网络请求能力 */
  httpRequest: (input: RequestInfo | URL, init?: RequestInit & ClientOptions) => Promise<Response>;
}
```

- `uploadViaBackend`：适用于调用后端统一上传命令，支持 `binary`/`form`/`base64` 等格式
- `logger`：输出日志到 Tauri 后端
- `httpRequest`：可直接发起跨域 HTTP 请求并继承应用代理设置

---

## 四、返回结果类型

```ts
interface PluginUploadResult {
  url: string;          // 上传后可访问链接
  deleteId: string;     // 删除时使用的标识
  metadata?: Record<string, unknown>; // 可选附加信息
}

interface PluginDeleteResult {
  success: boolean;
  message?: string;     // 可选错误或成功提示
}
```