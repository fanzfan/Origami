# Repository Guidelines

本指南适用于仓库内所有编码代理。Origami 是基于 Tauri 2、Rust、React 18、TypeScript 和 Vite 的跨平台压缩文件管理器。共享规则维护在本文件；`CLAUDE.md` 通过引用本文件补充架构说明。

## 目录与职责

- `src/App.tsx`：主窗口、归档任务和全局快捷键；`src/main.tsx`：按查询参数选择主窗口、`MiniProgress` 或 `AskDialog`，并应用各窗口共用设置。
- `src/components/`：文件浏览、归档浏览和各类对话框；`src/api.ts`：Tauri 命令与事件的类型封装；`src/settings.ts`：设置持久化。
- `src/i18n/`：语言解析与中英文资源；`src/styles.css`：共用样式及平台、窗口差异。
- `src-tauri/src/lib.rs`：命令注册、平台分派、任务与窗口生命周期；`archive/`：格式检测、列表、解压、创建、编辑、预览及分卷处理。
- `src-tauri/src/` 下的平台模块负责文件关联、系统菜单、认证、凭据及窗口效果；`src-tauri/tests/` 存放 Rust 集成测试。
- `src-tauri/tauri.conf.json`：应用、窗口与打包配置；`src-tauri/capabilities/default.json`：窗口和插件权限。
- `finder-extension/`：macOS FinderSync 扩展；`windows-extension/`：Windows 11 顶层右键菜单及稀疏包，操作前阅读其 `README.md`。

## 开发与验证

以下命令均从仓库根目录执行，兼容 PowerShell：

```sh
npm install
npm run tauri dev
npm run build
npm run tauri build
```

`npm run dev` 仅启动前端服务器（固定端口 `1420`）；完整桌面调试使用 `npm run tauri dev`。直接 `cargo run` 的开发二进制也依赖该服务器。

**每次修改后都必须执行：**

```sh
npx tsc --noEmit
cargo check --manifest-path src-tauri/Cargo.toml
```

修改 Rust 行为时，按范围补充验证：

```sh
cargo test --manifest-path src-tauri/Cargo.toml --lib
cargo test --manifest-path src-tauri/Cargo.toml --test engine
cargo test --manifest-path src-tauri/Cargo.toml
```

- `engine` 覆盖创建、列表、解压、编辑及加密等归档往返行为。修改 `CreateOptions`、`ExtractOptions`、`ListOptions` 或归档函数签名时，同步更新测试辅助函数；`cargo check` 不编译测试。
- `services` 测试仅在 macOS 执行，依赖 `plutil`；其他平台的零测试结果不代表验证通过。
- Windows 集成测试依赖 `src-tauri/build.rs` 注入 Common Controls v6 manifest，避免测试程序因缺少 `TaskDialogIndirect` 在启动时退出。
- 当前没有前端测试脚本或独立 lint/format 脚本。界面变更需在桌面端手动检查；报告实际执行的检查及未覆盖的平台。

## 架构与数据契约

- 新增或修改命令时，同步维护 `lib.rs` 中的命令定义、`tauri::generate_handler!` 注册及 `src/api.ts` 封装；Rust 的 `camelCase` 序列化结构与 TypeScript 类型也须同步。
- 跨平台命令保持统一接口，由 `#[cfg]` 分派到平台模块；不支持的平台返回安全默认值。平台依赖放在 `[target.'cfg(...)'.dependencies]` 下。
- 主窗口以 `visible: false` 启动，挂载后调用 `frontend_ready`。无交互快捷任务由 `mini` 驱动，需要配置的任务由 `ask` 驱动。不要依赖隐藏的主窗口执行任务，也不要让多个窗口争抢会清空队列的 `take_pending_actions`。
- 新增窗口标签或前端窗口、插件 API 调用时，核对 `capabilities/default.json` 的授权；`main.tsx` 中的共用初始化也要覆盖辅助窗口。
- 解压、压缩等阻塞操作使用 `spawn_blocking`，通过 `Jobs`、`cancel_job` 和 `job-progress` 协作；事件监听必须按 `jobId` 过滤。保留 `PASSWORD_REQUIRED`、`CANCELLED` 错误标记，前端依赖它们选择交互流程。
- 格式识别遵循内容优先于扩展名的逻辑，保留分卷检测和文件名编码处理。解压路径必须经过 `sanitize_rel_path`；写入归档条目名使用 `/` 分隔符，避免 Windows 路径产生反斜杠条目。
- 单文件“用默认程序打开”通过 `extract_entry_to_temp` 解压到 `app_cache_dir`，再交给 opener。

## 密码与系统认证

- 明文密码只写入系统凭据库，服务名为 `dev.vela.origami.passwords`。`passwords.json` 只存储 id、备注和时间戳等索引元数据；读取或写入时自动迁移旧明文，删除索引项时同步删除凭据。
- 展示明文前调用 `system_auth`：认证不可用时允许继续；失败或用户取消时不展示；认证机制异常时保留合理回退，避免把用户锁在功能之外。
- 保留 `LazyPasswords` 的延迟读取行为：先尝试显式密码，失败后才读取凭据库，避免打开普通归档时触发钥匙串提示。

## 跨平台交付

`cargo check` 仅覆盖当前目标；修改平台分支后，须在对应平台完成编译与运行验证。缺少目标 C 工具链导致的交叉编译失败不能直接判为应用缺陷。

- **Windows**：需要 Rust MSVC 工具链、WebView2 Runtime 和包含 C++ 工作负载的 Visual Studio Build Tools。
  - `winassoc.rs`：验证切换文件关联及恢复 Explorer 默认打开程序。
  - `winmenu.rs`：验证经典右键菜单安装后能启动 Origami，移除后注册表项清理完成。
  - `sysauth.rs` / `passwords.rs`：覆盖 Windows Hello 成功、取消、不可用路径，以及凭据写入、迁移、删除和索引无明文。
  - `App.tsx`：验证 `Ctrl + ,` 与 `Ctrl + +/-/0`。
  - `windows-extension/`：按其 README 构建、签名和注册后，验证 Windows 11 顶层菜单；经典菜单验证不能替代此项。
- **macOS**：打包版与开发版共用 bundle id；若 LaunchServices 唤起 `/Applications/Origami.app`，先安装最新构建产物再验证。
  - `macassoc.rs`：验证文件关联写入、取消和回退处理程序。
  - `services.rs`：验证 Finder Quick Action 安装、移除，以及压缩、解压深链接的目标路径。
  - `sysauth.rs` / `passwords.rs`：验证系统认证、Keychain 读写、旧数据迁移和删除。
  - 验证 `Cmd + ,` 与 `Cmd + +/-/0`。
  - 包含 FinderSync 扩展的构建使用 `npm run build:mac`；先按 `scripts/setup_local_signing.sh` 配置本地签名，普通 `tauri build` 的 DMG 不包含该扩展。

## 国际化与代码约定

- 注释和开发文档使用中文。新增界面文案从 `src/i18n/locales/` 的资源键读取，同步维护 `zh-CN.ts` 与 `en-US.ts`，使用稳定语义键及 i18next 插值、复数能力；不要使用中文原文作键或在 JSX 中拼接可翻译句子。
- 语言偏好支持 `system`、`zh-CN`、`en-US`；新增语言时同步扩展资源、语言类型及设置选项。迁移可分批进行，但同一功能区域需完整覆盖，避免单个对话框中英混杂；检查两种语言的布局、按钮宽度和空状态。
- 快捷键修饰键按平台选择：macOS 使用 `metaKey`，Windows/Linux 使用 `ctrlKey`。
- 遵循现有代码风格并优先修改已有文件；仅为资源、平台模块或职责清晰分离的内容新增文件。
- 修改前检查工作区状态，保留已有改动；不覆盖任务范围外内容，不手工修改 `dist/`、`src-tauri/target/`、`src-tauri/gen/` 等生成产物。
