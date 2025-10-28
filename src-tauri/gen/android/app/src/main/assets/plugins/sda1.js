export default {
  name: 'sda1 图床',
  description: '通过 p.sda1.dev 的 external_noform 接口上传文件（支持图像和任意文件）',
  // 不强制参数，插件接受任意文件类型
  parameters: [],
  supportedFileTypes: [],

  /**
   * 上传文件。
   * - filePath: 本地文件绝对路径
   * - originalFileName: 原始文件名
   * - params: 用户在界面填写的参数（本插件无）
   * - context: 运行时上下文（uploadViaBackend, httpRequest, logger）
   */
  upload: async function (filePath, originalFileName, params, context) {
    const logger = context.logger ?? console;
    const name = originalFileName || 'file';
    const uploadUrl = `https://p.sda1.dev/api/v1/upload_external_noform?filename=${encodeURIComponent(
      name
    )}`;

    logger.info?.(`[sda1] upload start -> ${uploadUrl}`);

    try {
      const resp = await context.uploadViaBackend({
        filePath,
        format: 'binary',
        config: {
          url: uploadUrl,
        },
      });

      // 尝试解析后端返回的 JSON（优先使用 body，如果不可用则用 rawText）
      let parsed = null;
      if (resp && resp.body) {
        parsed = resp.body;
      }
      if (!parsed) {
        try {
          parsed = resp.rawText ? JSON.parse(String(resp.rawText)) : null;
        } catch (err) {
          // ignore
        }
      }

      if (!parsed) {
        const msg = `sda1: 无法解析上传响应`;
        logger.error?.(msg);
        throw new Error(msg);
      }

      const data = parsed.data ?? parsed;
      const url = data?.url;
      const deleteUrl = data?.delete_url ?? data?.deleteUrl ?? data?.deleteURL;

      if (!url) {
        const msg = `sda1: 上传响应中未包含 data.url`; 
        logger.error?.(msg, parsed);
        throw new Error(msg);
      }

      logger.info?.(`[sda1] upload success -> ${url}`);

      return {
        url: String(url),
        deleteId: deleteUrl ? String(deleteUrl) : '',
        metadata: parsed,
      };
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      logger.error?.(`[sda1] upload failed: ${message}`);
      throw err;
    }
  },

  /**
   * 删除文件。
   * - deleteId: 我们在 upload 返回时放在 deleteId 字段中的 URL
   */
  remove: async function (deleteId, context) {
    const logger = context.logger ?? console;
    if (!deleteId) {
      const msg = 'sda1: 缺少删除 URL';
      logger.warn?.(msg);
      return { success: false, message: msg };
    }

    try {
      logger.info?.(`[sda1] delete request -> ${deleteId}`);
      // 尝试用 DELETE 方法调用删除 URL；如果服务器需要 POST，可根据返回再调整
      let res;
      try {
  // 目标服务要求使用 GET 方式来触发删除（根据测试），因此使用 GET
  res = await context.httpRequest(deleteId, { method: 'GET' });
      } catch (reqErr) {
        // 明确记录请求抛出的异常，并返回给调用方便于分析
        const errMsg = reqErr instanceof Error ? reqErr.message : String(reqErr);
        logger.error?.('[sda1] delete request threw an error', reqErr);
        return { success: false, message: 'delete request threw', debug: { error: errMsg } };
      }

      // 记录更多调试信息 —— 如果 res 为 null/undefined 专门标记
      try {
        if (res == null) {
          logger.warn?.('[sda1] delete response is empty (null/undefined)');
          return { success: false, message: 'empty delete response', debug: { responsePresent: false, typeof: typeof res, value: res } };
        }
        // 打印类型与键名，避免大型对象被控制台截断
        const info = { typeof: typeof res, keys: Object.keys(res instanceof Object ? res : {}) };
        logger.debug?.('[sda1] delete response', info);
      } catch (e) {
        // ignore logging errors
      }

      // 尝试判断是否成功
      let ok = false;
      try {
        // tauri plugin-http 的 Response 可能包含 ok 或 status
        if (res && typeof res === 'object') {
          if (typeof res.ok === 'boolean') ok = res.ok;
          if (!ok && typeof res.status === 'number') ok = res.status >= 200 && res.status < 300;
        }
      } catch (e) {
        // ignore
      }

      if (!ok) {
        // 尝试解析返回体看是否包含 success 字段
        try {
          const text = res?.data ?? res?.rawText ?? null;
          if (text) {
            const parsed = typeof text === 'string' ? JSON.parse(text) : text;
            if (parsed && (parsed.success === true || parsed.ok === true)) ok = true;
          }
        } catch (e) {
          // ignore
        }
      }

      if (ok) {
        logger.info?.(`[sda1] delete success -> ${deleteId}`);
        return { success: true, debug: { status: res?.status, data: res?.data ?? res?.rawText ?? null } };
      }

      const msg = `sda1: 删除接口返回非成功状态`;
      logger.warn?.(msg, res);
      return { success: false, message: msg, debug: { status: res?.status, data: res?.data ?? res?.rawText ?? null } };
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      logger.error?.(`[sda1] delete failed: ${message}`);
      return { success: false, message, debug: { error: message } };
    }
  },
};
