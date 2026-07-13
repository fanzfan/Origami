# AGENTS.md

面向在本仓库工作的 Codex 实例。Origami 是 Tauri 2 + Rust + React/TypeScript 的跨平台压缩包管理器。

## 校验命令

改动后务必跑通：

```bash
npx tsc --noEmit
cd src-tauri && cargo check
```

运行和构建：

```bash
npm run tauri dev
npm run tauri build
```

## 跨平台交付

- 平台依赖放入 `[target.'cfg(...)'.dependencies]`，平台代码使用 `#[cfg(target_os = "...")]` 隔离。
- `cargo check` 只覆盖当前目标；修改平台专属分支后，应在对应系统上补做编译与运行验证。不要把缺少目标平台 C 工具链导致的交叉编译失败当成业务代码错误。
- Windows 环境需要 Rust MSVC 工具链、WebView2 Runtime、Visual Studio Build Tools（C++ 工作负载）。
- macOS 的打包版与开发版 bundle id 相同。LaunchServices 若将 `/Applications/Origami.app` 带到前台，应以最新 `npm run tauri build` 产物覆盖安装后再验证。

涉及 Windows 分支时重点验证：

- `winassoc.rs`：勾选/取消文件关联后，资源管理器默认程序正确变更和还原。
- `winmenu.rs`：经典右键菜单安装后可启动 Origami，移除后注册表项消失。
- `sysauth.rs`：Windows Hello 成功、取消和不可用三条路径都不会泄露明文或锁死用户。
- `passwords.rs`：`passwords.json` 仅含 id、备注和时间戳；凭据 service 为 `dev.vela.origami.passwords`；删除时同步删除凭据。
- `App.tsx`：`Ctrl + ,` 与 `Ctrl + +/-/0` 正常。
- `windows-extension/`：按 README 构建、签名和注册后验证 Win11 新版顶层菜单。

涉及 macOS 分支时重点验证：

- `macassoc.rs`：文件关联写入、取消和回退处理器正确。
- `services.rs`：Finder Quick Action 安装/移除、压缩/解压深链与目标路径正确。
- `sysauth.rs` / `passwords.rs`：系统认证与钥匙串读写、迁移、删除正常。
- `Cmd + ,` 与 `Cmd + +/-/0` 正常。

## 架构

- **命令注册**：后端命令在 `src-tauri/src/lib.rs` 定义并在 `tauri::generate_handler!` 注册；前端在 `src/api.ts` 封装。三处必须一致。
- **平台分发**：跨平台命令在 `lib.rs` 暴露统一接口，再按 `#[cfg]` 分发到平台模块；不支持的平台返回安全默认值。
- **窗口显隐**：主窗口初始 `visible: false`，前端挂载后调用 `frontend_ready`；无需交互的快捷任务由 `mini` 窗口驱动，需要设置的任务由 `ask` 窗口驱动。
- **重活异步化**：解压、压缩等阻塞任务使用 `spawn_blocking`，通过 job、进度事件和取消命令与前端协作。
- **密码门控**：展示明文前先调用 `system_auth`；认证不可用时放行，认证失败或用户取消时不显示，认证机制异常时不能把用户锁死。
- **密码存储**：明文只写入系统凭据库；`passwords.json` 只保存索引。旧版明文在加载和写入入口自动迁移。

## 国际化

- 注释和开发文档使用中文；用户可见文案必须通过 `src/i18n/locales/` 的资源键提供，不再直接写死中文。
- 当前支持 `zh-CN` 与 `en-US`，语言偏好还包含 `system`。新增语言时同步扩展资源、语言类型和设置选项。
- 使用稳定的语义 key 与 i18next 插值/复数能力，不以中文原文为 key，不在 JSX 中拼接可翻译句子。
- 每次迁移文案同时维护中英文；允许分批迁移旧界面，但同一功能区应完整迁移，避免一个弹窗内中英混杂。

## 代码约定

- 遵循现有代码风格，优先编辑既有文件；只有资源、平台模块或职责明确时才新建文件。
- 单文件“用默认程序打开”走 `extract_entry_to_temp` 解压到 `app_cache_dir`，再交给 opener。
- 快捷键修饰键按平台自适应：macOS 使用 `metaKey`，Windows/Linux 使用 `ctrlKey`。
- 保留并尊重工作区已有改动，不覆盖与当前任务无关的内容。
