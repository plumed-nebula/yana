# Yana

Yana 是一个基于 Tauri（Rust 后端）和 Vue 3 + TypeScript（前端）的桌面图像管理/上传工具。应用支持可插拔的图片上传后端（Image Host Plugin），并包含内置的 S3 后端实现。用户可以在运行时添加自定义 JS 插件来扩展支持的图像托管服务。

## 使用方法

详见 [使用方法 (USAGE.md)](./USAGE.md)

## 项目亮点
- 桌面原生应用（Tauri）结合现代前端（Vite + Vue 3）
- 可扩展的插件系统：在运行时加载/重载第三方 Image Host 插件（ESM 模块）
- 内置 S3 上传支持（后端签名和上传逻辑在 Rust/Tauri 后端实现）
- 支持上传前压缩，有丰富的压缩选项

## 技术栈
- 前端：Vue 3、TypeScript、Vite
- 后端：Tauri（Rust）
- 打包：Tauri bundle（跨平台）

## 快速开始（开发）
先确保你已安装 Node.js、Rust 与 Tauri 所需工具链。

1. 安装依赖

```powershell
npm install
```

2. 启动开发模式（会启动 Vite dev server 并运行 Tauri）

```powershell
npm run tauri dev
```

3. 构建发行包

```powershell
npm run tauri build
```

注意：在 Windows 上用 PowerShell 执行上述命令。

## 系统支持

- Windows 10 及以上（已测试 Windows 11，原生支持）

其它平台未经测试，macOS 未作标题栏适配，Linux 未进行窗口配置优化。

移动平台暂不支持。

## 插件系统（Image Host Plugin）

核心思路：插件是符合约定的 ESM 模块（`.js` / `.mjs`），导出一个包含 `upload` 与 `remove` 方法的对象（可用 `export default { ... }` 或 `module.exports` 风格）。应用通过后端命令 `list_image_host_plugins` 列出可用插件。

- 插件位置
	- 编译/发布时随应用打包的插件放在应用资源目录（Resource），作为内置或编译期插件。
	- 用户在运行时添加的插件会复制到用户配置目录下的 `plugins` 子目录（例如 Windows: `%APPDATA%/com.yana.dev/plugins`，Linux/macOS: `$XDG_CONFIG_HOME/com.yana.dev/plugins` 或 `~/.config/com.yana.dev/plugins`）。

- 加载流程（简要）
	1. 后端发现插件文件并返回每个插件的 `id` 与 `script`（对用户插件 `script` 为绝对文件路径）。
	2. 前端在 DEV/PROD 下通过不同方式解析可访问的 URL：
		 - DEV：相对路径由 dev server 提供；绝对文件路径会使用 Tauri 的 `convertFileSrc` 映射为可访问 URL，避免 Vite 返回 index.html 导致解析错误。
		 - PROD：使用 `convertFileSrc` 映射资源协议（asset://或tauri asset）
	3. 前端通过 `import(/* @vite-ignore */ url)` 动态加载插件；若 import 失败，会尝试 fetch 源码并通过 Blob URL 再次 import（作为回退）。

- 插件编写要点
	- 必须导出 `upload(filePath, originalFileName, params, context)` 与 `remove(deleteId, context)`，两者均为异步函数。
	- 如果插件内部存在相对导入（多文件插件），建议在安装时将插件打包为单文件（使用 esbuild/rollup）以避免加载时的相对路径问题。

## 常见问题与排查

- 问：加载插件时报错 "Failed to fetch dynamically imported module" 或 "Unexpected token '<'"。
	- 原因：在开发模式下请求指向了 dev server（例如 `http://localhost:1420/C:/Users/...`），vite 返回了 HTML（index.html），导致解析失败。解决方法：确保 `image_hosts` 返回合适的 script 值（绝对路径或相对资源路径），并且前端在 DEV 下对绝对路径调用 `convertFileSrc`。

- 问：运行时报 "asset protocol not configured to allow the path: C:\Users\..."。
	- 原因：Tauri 的 asset 协议对允许的磁盘路径有白名单。解决方法：已在 `tauri.conf.json` 的 `app.security.assetProtocol.scope` 中加入 `$APP_CONFIG/**` 与 `**/plugins/**`。如果仍报错，请把具体路径贴上，我会帮你加入更精确的匹配。

- 问：添加插件时复制到安装目录失败（权限错误）。
	- 说明：这是预期行为，用户安装目录通常不可写。应用会把用户添加的插件复制到用户配置目录（AppData / XDG），从该目录加载插件。

## 代码结构速览（高层）
- `src/`：前端 Vue 应用
	- `plugins/`：内置/示例插件源码（开发时）
	- `stores/`：Pinia 状态管理（包括 imageHosts 的 store）
	- `views/`、`components/` 等：UI 代码
- `src-tauri/`：Tauri 后端（Rust）
	- `src/image_hosts.rs`：插件发现、加载、添加插件、插件设置读写等逻辑
	- `src/s3.rs`：S3 上传/删除实现（在后端处理签名/上传）

## 开发者提示
- 如果你要开发或发布插件，请尽量将插件打包为单文件 ESM 模块，以保证在不同运行环境下的兼容性。
- 在调试插件加载时，开启开发控制台并观察 `registry.ts` 的日志，它会输出加载失败的 URL 与错误消息，能帮助定位是路径问题还是模块语法问题。

## 贡献与联系
- 欢迎提交 PR 或 issue。请在 PR 中说明你修改的范围并附上简单测试步骤。

## 插件开发指南

参加 [插件开发指南 (PLUGIN_DEVELOPMENT_GUIDE.md)](./PLUGIN_DEVELOPMENT_GUIDE.md) 了解如何编写和调试自定义图床插件。

## 图标

<div> Icons made by <a href="https://www.flaticon.com/authors/seyfdesigner" title="SeyfDesigner"> SeyfDesigner </a> from <a href="https://www.flaticon.com/" title="Flaticon">www.flaticon.com'</a></div>
