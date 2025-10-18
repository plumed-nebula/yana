/**
 * 重试工具函数 - 为异步操作提供自动重试能力
 * 支持自定义重试次数和延迟时间
 */

interface RetryOptions {
  maxRetries?: number;
  delayMs?: number;
}

/**
 * 执行异步操作，失败时自动重试
 * @param operation - 异步操作函数
 * @param options - 重试配置选项
 * @returns 操作结果
 * @throws 所有重试尝试都失败时抛出最后一次的错误
 */
export async function retryAsync<T>(
  operation: () => Promise<T>,
  options: RetryOptions = {}
): Promise<T> {
  const maxRetries = options.maxRetries ?? 1; // 默认重试1次
  const delayMs = options.delayMs ?? 0; // 默认无延迟

  let lastError: Error | unknown;

  for (let attempt = 0; attempt <= maxRetries; attempt++) {
    try {
      return await operation();
    } catch (error) {
      lastError = error;
      // 如果还有重试次数且不是最后一次，则等待后重试
      if (attempt < maxRetries && delayMs > 0) {
        await new Promise((resolve) => setTimeout(resolve, delayMs));
      }
    }
  }

  throw lastError;
}

/**
 * 为给定的异步函数创建一个带重试能力的包装函数
 * @param fn - 要包装的异步函数
 * @param options - 重试配置选项
 * @returns 包装后的函数
 */
export function withRetry<T extends (...args: any[]) => Promise<any>>(
  fn: T,
  options: RetryOptions = {}
): T {
  return (async (...args: any[]) => {
    return retryAsync(() => fn(...args), options);
  }) as T;
}
