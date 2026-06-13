# CLAUDE.md

面向在本仓库工作的 Claude 实例的指引。Origami 是 Tauri 2 + Rust + React/TS 的跨平台压缩包管理器。

## 校验命令

改动后务必跑通：

```bash
npx tsc --noEmit                  # 前端类型检查
cd src-tauri && cargo check       # 后端编译检查（在 src-tauri 目录）
```

运行 / 构建：

```bash
npm run tauri dev                 # 开发模式
npm run tauri build               # 发行构建（.app / .dmg 等）
```

`cargo` 不在默认 PATH 时：`export PATH="$HOME/.cargo/bin:$PATH"`。

## 跨平台交付约束（重要）

**开发机是 macOS，无法编译/测试 Windows 产物。** 对 Windows 功能：

- 编写完整的 Rust 代码 + 构建脚本，由用户在 Windows 上自行验证。
- 用 `[target.'cfg(target_os = "windows")'.dependencies]` 隔离平台依赖，用 `#[cfg(target_os = "...")]` 隔离平台代码。
- 保证 **macOS 构建仍能编译**：`cargo check`（darwin 目标）+ `tsc` 必须干净。
- 不要尝试 `cargo check --target x86_64-pc-windows-msvc`——C 工具链（liblzma 等）无法交叉编译，这是预期限制，不是代码错误。

### 在 Windows 上验证（交给用户执行）

环境：Rust MSVC 工具链、WebView2 Runtime、VS Build Tools（C++ 工作负载）。命令：`npx tsc --noEmit`、`cd src-tauri; cargo check`、`npm run tauri dev` / `npm run tauri build`。Windows 没有 macOS 的「旧 app 抢前台」坑，`tauri dev` 即所见即当前代码。

每次涉及 Windows 分支的改动，提示用户按此清单实测：

- **文件关联**（`winassoc.rs`）：关联面板勾选/取消 → 资源管理器默认程序随之变更并能还原。
- **经典右键菜单**（`winmenu.rs`）：安装后文件右键出现 Origami 项并能打开；移除后干净消失。
- **Windows Hello 密码门控**（`sysauth.rs`）：查看明文前弹 Hello；取消则不显示；无 Hello 不被锁死。
- **快捷键**（`App.tsx`）：`Ctrl + ,` 开设置；`Ctrl + +/-/0` 调字号/复位。
- **Win11 新版顶层菜单**（`windows-extension/`）：跑 `build.ps1` 并按其 README 完成签名注册后验证。

## 架构

- **命令注册**：后端命令在 `src-tauri/src/lib.rs` 用 `#[tauri::command]` 定义并在 `tauri::generate_handler!` 注册；前端在 `src/api.ts` 用 `invoke` 封装后供组件调用。三处要一致。
- **平台分发模式**：跨平台能力（系统认证、文件关联）在 `lib.rs` 暴露统一命令，内部按 `#[cfg]` 分发到 `sysauth.rs` / `macassoc.rs` / `winassoc.rs`；不支持的平台给出安全的默认行为。
- **窗口显隐**：主窗口 `visible: false`，前端挂载后调用 `frontend_ready` 命令才 `show()`；快捷压缩启动时改用 `mini` 进度窗口。
- **重活异步化**：解压/压缩等阻塞操作用 `spawn_blocking`，通过 job + 进度事件回传前端，可取消。
- **密码门控**：密码管理器展示明文前先 `system_auth`（Touch ID / Windows Hello）；认证不可用时放行，认证出错时不要把用户锁死在外。

## 约定

- UI、注释、用户可见文案用中文。
- 遵循现有代码风格；优先编辑既有文件，不无故新建。
- 单文件「用默认程序打开」走 `extract_entry_to_temp` 解压到 `app_cache_dir` 再用 opener 打开。
- 快捷键修饰键按平台自适应：macOS 用 Cmd（`metaKey`），Windows/Linux 用 Ctrl（`ctrlKey`），与主流应用习惯一致。前端用 `navigator.platform`/`userAgent` 检测，见 `App.tsx` 的 keydown handler。

## 本地测试的坑

`npm run tauri dev` 会被**已安装的 `/Applications/Origami.app`** 干扰：macOS LaunchServices 会按 bundle id（`dev.vela.origami`）把已安装的那份带到前台，于是看到的是旧 UI 而非开发版。要可靠地在本机验证当前代码，最稳的做法是 `npm run tauri build` 后把新产物装到 `/Applications` 再测，而不是和 dev server 的窗口显隐/缓存较劲。
