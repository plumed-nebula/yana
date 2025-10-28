/**
 * freeimage.host 图床插件
 * 使用 freeimage.host API 进行上传与删除操作。
 */
export const name = 'freeimage.host';
export const description = '使用 freeimage.host API 进行上传与删除操作。';

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
    description: '通用图片格式',
  },
];

export const parameters = [
  {
    key: 'key',
    label: 'API Key',
    type: 'password',
    required: true,
    description: '在 freeimage.host 获取的 API Key',
  },
];

export async function upload(filePath, originalFileName, params, context) {
  const apiKey = String(params.key ?? '').trim();
  if (!apiKey) {
    const msg = 'freeimage.host: 缺少 API Key';
    context.logger.error(msg);
    throw new Error(msg);
  }
  context.logger.info('freeimage.host: 开始上传');
  const resp = await context.uploadViaBackend({
    filePath,
    format: 'form',
    config: {
      url: 'https://freeimage.host/api/1/upload',
      fieldName: 'source',
      additionalFields: { key: apiKey, action: 'upload', format: 'json' },
      fileName: originalFileName,
      timeoutMs: 60_000,
    },
  });
  // 解析响应
  let body = null;
  if (resp.body && typeof resp.body === 'object') {
    body = resp.body;
  } else if (resp.rawText) {
    try {
      body = JSON.parse(resp.rawText);
    } catch (e) {
      // ignore
    }
  }
  if (!body || body.status_code !== 200 || !body.image || !body.image.url) {
    const msg = 'freeimage.host: 上传失败';
    context.logger.error(msg, body);
    throw new Error(msg);
  }
  const url = String(body.image.url);
  const idEncoded = String(body.image.id_encoded);
  context.logger.info('freeimage.host: 上传成功 ->', url);
  return {
    url,
    deleteId: JSON.stringify({ key: apiKey, id: idEncoded }),
    metadata: body,
  };
}

export async function remove(deleteId, context) {
  // freeimage.host 不支持删除 API，删除操作直接标记成功
  context.logger.info('freeimage.host: 删除接口不可用，noop 返回成功');
  return { success: true };
}
