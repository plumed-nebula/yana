/**
 * S3 预签名上传插件
 * 使用用户提供的 S3 PUT 预签名 URL 进行文件上传。
 */
export const name = 'S3 上传';
export const version = '1.0.0';
export const author = '';
export const description = '使用 S3 预签名 URL 进行上传';

export const supportedFileTypes = [
  {
    mimeTypes: [],
    extensions: [],
    description: '所有文件类型（取决于 S3 存储桶配置）',
  },
];

export const parameters = [
  {
    key: 'uploadUrl',
    label: '预签名 PUT URL',
    type: 'text',
    required: true,
    description: '在 S3 控制台或后端生成的预签名 PUT URL',
  },
];

export async function upload(filePath, originalFileName, params, context) {
  const url = String(params.uploadUrl || '').trim();
  if (!url) {
    const msg = 'S3 上传需要有效的预签名 URL';
    context.logger.error(msg);
    throw new Error(msg);
  }
  context.logger.info('S3 上传开始 ->', url);
  const resp = await context.uploadViaBackend({
    filePath,
    format: 'binary',
    config: {
      url,
      timeoutMs: 120_000,
    },
  });
  // 检查状态
  const status = resp.status ?? (Array.isArray(resp.headers) ? null : null);
  if (status != null && (status < 200 || status >= 300)) {
    const msg = `S3 上传失败: HTTP ${status}`;
    context.logger.error(msg, resp);
    throw new Error(msg);
  }
  // 生成访问 URL（去除 query）
  const publicUrl = url.split('?')[0];
  context.logger.info('S3 上传成功 ->', publicUrl);
  return { url: publicUrl, deleteId: '', metadata: resp };
}

export async function remove(deleteId, context) {
  // 无删除操作
  context.logger.info('S3 删除无操作 (noop)');
  return { success: true };
}
