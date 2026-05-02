# Yorling

[English](README.md) | [简体中文](README.zh-CN.md)

Yorling 是一款优先面向 macOS 的本地 AI 与效率工作台。它把键盘映射、Agent 终端启动、Finder 操作、剪贴板历史，以及悬浮的 Agent 活动岛整合到一个 Tauri 桌面应用里。

## 功能

- 基于 macOS 原生拦截能力的键盘工作流层。
- 面向项目路径的多终端与 Agent 启动器。
- 通过 provider hooks 接入实时状态的 Dynamic Island 风格活动面板。
- 基于 Finder Sync 的 Finder 原生右键菜单操作。
- 支持文字、图片、文件、文件夹和分组剪贴内容的剪贴板历史。
- 键盘音乐与节奏练习工具。

## 技术栈

- macOS 侧使用 Tauri 2、Rust、Swift、AppKit 和 Finder Sync。
- 桌面 UI 使用 React 19、TypeScript、Vite、Zustand 和 CSS。
- Rust workspace 拆分核心类型、键盘引擎、macOS 平台层、Island core 和 hook bridge。

## 开发

安装依赖：

```sh
pnpm install
```

运行 Web UI：

```sh
pnpm dev
```

运行 macOS app 开发流程：

```sh
pnpm run dev:macos-app
```

运行测试：

```sh
pnpm test
cargo test --workspace
```

构建前端：

```sh
pnpm build
```

## macOS 说明

部分功能需要 macOS 权限，例如辅助功能、自动化、屏幕录制和 Finder Sync 扩展授权。应用应在功能需要权限时，引导用户跳转到对应设置。

Finder Sync 打包由下面的命令处理：

```sh
pnpm run build:finder-sync
```

## 隐私

Yorling 面向本地桌面工作流设计。请不要提交本地 hook 导出、个人路径、应用支持目录文件、日志、token 或生成的构建产物。

## 开源协议

Yorling 使用 MIT 协议开源。详情见 [LICENSE](LICENSE)。
