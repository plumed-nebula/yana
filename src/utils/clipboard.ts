// 尝试使用 Tauri 的 clipboard API；若不可用则回退到浏览器的 navigator.clipboard
export async function writeText(text: string): Promise<void> {
    // 仅使用 clipboard-manager 插件（前端插件包），若不可用再回退到浏览器 API
    try {
        const plugin = await import('@tauri-apps/plugin-clipboard-manager');
        const impl = plugin as any;
        if (impl) {
            if (typeof impl.writeText === 'function') {
                await impl.writeText(text);
                return;
            }
            if (typeof impl.write === 'function') {
                await impl.write(text);
                return;
            }
            if (typeof impl.copy === 'function') {
                await impl.copy(text);
                return;
            }
            if (typeof impl.setText === 'function') {
                await impl.setText(text);
                return;
            }
        }
    } catch (e) {
        // ignore and fallback to navigator
    }
    if (navigator.clipboard && navigator.clipboard.writeText) {
        await navigator.clipboard.writeText(text);
        return;
    }
    throw new Error('剪贴板写入不可用');
}

export async function readText(): Promise<string> {
    try {
        const plugin = await import('@tauri-apps/plugin-clipboard-manager');
        const impl = plugin as any;
        if (impl) {
            if (typeof impl.readText === 'function') {
                const t = await impl.readText();
                if (t != null) return t;
            }
            if (typeof impl.read === 'function') {
                const t = await impl.read();
                if (t != null) return t;
            }
            if (typeof impl.paste === 'function') {
                const t = await impl.paste();
                if (t != null) return t;
            }
        }
    } catch (e) {
        // ignore
    }
    if (navigator.clipboard && navigator.clipboard.readText) {
        return await navigator.clipboard.readText();
    }
    throw new Error('剪贴板读取不可用');
}

// 读取剪贴板中的图片（优先尝试插件），返回 Blob 或 null
export async function readImage(): Promise<Blob | null> {
    // 1) 尝试动态导入 clipboard-manager 插件（如果存在），并调用可能的 readImage 方法
    try {
        // 插件 API 可能返回 data URL 或 base64 字符串，做一定的兼容处理
        // eslint-disable-next-line @typescript-eslint/no-var-requires
        const plugin = await import('@tauri-apps/plugin-clipboard-manager');
        if (plugin && typeof (plugin as any).readImage === 'function') {
            const res = await (plugin as any).readImage();
            if (!res) return null;
            if (typeof res === 'string') {
                // 如果是 data URL 或 base64
                const dataUrl = res.startsWith('data:') ? res : `data:image/png;base64,${res}`;
                const fetched = await fetch(dataUrl);
                return await fetched.blob();
            }
            // 如果是 ArrayBuffer / Uint8Array
            if (res instanceof ArrayBuffer || ArrayBuffer.isView(res)) {
                const arr = res instanceof ArrayBuffer ? new Uint8Array(res) : new Uint8Array((res as any).buffer ?? res);
                return new Blob([arr as any]);
            }
            // 如果返回对象里有 data 或 bytes 字段
            if (res.data) {
                const maybe = res.data;
                if (typeof maybe === 'string') {
                    const dataUrl = maybe.startsWith('data:') ? maybe : `data:image/png;base64,${maybe}`;
                    const fetched = await fetch(dataUrl);
                    return await fetched.blob();
                }
                if (maybe instanceof ArrayBuffer || ArrayBuffer.isView(maybe)) {
                    const arr = maybe instanceof ArrayBuffer ? new Uint8Array(maybe) : new Uint8Array((maybe as any).buffer ?? maybe);
                    return new Blob([arr as any]);
                }
            }
        }
    } catch (e) {
        // ignore and fallback to navigator
    }

    // 2) 回退到浏览器 clipboard.read()（仅在支持时）
    try {
        if (navigator.clipboard && typeof (navigator.clipboard as any).read === 'function') {
            const items = await (navigator.clipboard as any).read();
            for (const item of items) {
                const type = item.types.find((t: string) => t.startsWith('image/'));
                if (type) {
                    const blob = await item.getType(type);
                    return blob;
                }
            }
            return null;
        }
    } catch (e) {
        // ignore
    }

    return null;
}
