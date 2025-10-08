/**
 * SM.MS 官方图床上传插件
 *
 * 设计说明：
 * - 上传使用后端封装的 uploadViaBackend，统一走 Rust 侧以避免前端权限问题；
 * - 删除操作会将图床返回的 hash 与当前令牌编码进 deleteId，保持标识的唯一性和自描述；
 * - 插件导出的结构遵循 `src/types/imageHostPlugin.ts` 中的接口约定。
 */

/** 插件名称 */
export const name = 'SM.MS 图床';
/** 插件版本号 */
export const version = '1.0.0';
/** 插件作者，可根据实际情况调整 */
export const author = 'Null';
/** 插件功能描述 */
export const description = '使用 SM.MS API v2 进行图床上传与删除操作。';

/**
 * 插件支持的文件类型声明：
 * - SM.MS 官方限制为常见图片类型；
 * - 这里列出常见 MIME 与扩展名供前端筛选使用。
 */
export const supportedFileTypes = [
  {
    mimeTypes: [
      'image/jpeg',
      'image/png',
      'image/gif',
      'image/webp',
      'image/bmp',
      'image/svg+xml',
    ],
    extensions: ['jpg', 'jpeg', 'png', 'gif', 'webp', 'bmp', 'svg'],
    description: '支持的图片格式（SM.MS 官方限制）',
  },
];

/**
 * 插件所需的参数清单：
 * - token: SM.MS 后台获取的 API Token，上传/删除接口均需携带。
 */
export const parameters = [
  {
    key: 'token',
    label: 'API Token',
    type: 'password',
    required: true,
    description: '请填写在 SM.MS 用户中心获取的 API Token，用于鉴权。',
  },
];

/**
 * 工具函数：提取并校验配置中的 token。
 * @param {Record<string, unknown>} params 用户配置参数
 * @param {import('../types/imageHostPlugin').PluginRuntimeContext} context 运行时上下文
 * @returns {string} 非空的 Token 字符串
 */
function resolveToken(params, context) {
  const token = params?.token != null ? String(params.token).trim() : '';
  if (!token) {
    const message = 'SM.MS 插件需要有效的 API Token 才能工作，请先在设置中填写。';
    context.logger.error(message);
    throw new Error(message);
  }
  return token;
}

/**
 * 上传函数实现。
 * @param {string} filePath 选中的本地文件绝对路径
 * @param {Record<string, unknown>} params 用户填写的参数
 * @param {import('../types/imageHostPlugin').PluginRuntimeContext} context 运行时能力封装
 * @returns {Promise<import('../types/imageHostPlugin').PluginUploadResult>}
 */
export async function upload(filePath, params, context) {
  const token = resolveToken(params, context);

  context.logger.info('开始向 SM.MS 上传图片');

  const response = await context.uploadViaBackend({
    filePath,
    format: 'form',
    config: {
      url: 'https://sm.ms/api/v2/upload',
      headers: {
        Authorization: token,
      },
      fieldName: 'smfile',
      timeoutMs: 60_000,
    },
  });

  const body = isRecord(response.body) ? response.body : null;
  if (!body || body.success !== true) {
    const message = body?.message || 'SM.MS 上传失败';
    context.logger.error(`SM.MS 上传失败: ${message}`);
    throw new Error(message);
  }

  const data = isRecord(body.data) ? body.data : null;
  const imageUrl = typeof data?.url === 'string' ? data.url : null;
  const deleteHash = typeof data?.hash === 'string' ? data.hash : null;

  if (!imageUrl || !deleteHash) {
    const fallback = response.rawText || 'SM.MS 返回数据缺失，无法获取图片链接或删除标识。';
    context.logger.error(fallback);
    throw new Error(fallback);
  }

  const deletePayload = JSON.stringify({ hash: deleteHash, token });

  context.logger.info('SM.MS 上传成功');

  return {
    url: imageUrl,
    deleteId: deletePayload,
    metadata: {
      page: typeof data.page === 'string' ? data.page : undefined,
      size: typeof data.size === 'number' ? data.size : undefined,
      width: typeof data.width === 'number' ? data.width : undefined,
      height: typeof data.height === 'number' ? data.height : undefined,
    },
  };
}

/**
 * 删除函数实现。
 * deleteId 中包含了上传时返回的 hash 和 token，避免再次请求用户输入。
 * @param {string} deleteId 上传阶段返回的删除标识
 * @param {import('../types/imageHostPlugin').PluginRuntimeContext} context 运行时能力封装
 * @returns {Promise<import('../types/imageHostPlugin').PluginDeleteResult>}
 */
export async function remove(deleteId, context) {
  try {
    const payload = JSON.parse(deleteId);
    const hash = typeof payload?.hash === 'string' ? payload.hash : '';
    const token = typeof payload?.token === 'string' ? payload.token : '';

    if (!hash || !token) {
      const message = 'SM.MS 删除信息不完整，无法执行删除操作。';
      context.logger.warn(message);
      return { success: false, message };
    }

    const response = await context.httpRequest(
      `https://sm.ms/api/v2/delete/${encodeURIComponent(hash)}`,
      {
        method: 'GET',
        headers: {
          Authorization: token,
        },
      }
    );

    const result = await safeJson(response);
    const success = result?.success === true;
    const message = typeof result?.message === 'string' ? result.message : undefined;

    if (!success) {
      const failMessage = message || 'SM.MS 删除失败';
      context.logger.warn(`SM.MS 删除失败: ${failMessage}`);
      return { success: false, message: failMessage };
    }

    context.logger.info('SM.MS 删除成功');
    return { success: true, message: message || '删除成功' };
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    context.logger.error(`SM.MS 删除异常: ${message}`);
    return { success: false, message };
  }
}

/**
 * 判断传入值是否为普通对象。
 * @param {unknown} value 
 * @returns {value is Record<string, any>}
 */
function isRecord(value) {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

/**
 * 安全地解析 Response 为 JSON，失败时返回 null。
 * @param {Response} response
 * @returns {Promise<Record<string, any> | null>}
 */
async function safeJson(response) {
  try {
    return await response.json();
  } catch (error) {
    return null;
  }
}
