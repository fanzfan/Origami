# Origami

跨平台压缩包管理器，对标 Bandizip / 7-Zip 的日常体验。基于 **Tauri 2 + Rust + React/TypeScript** 构建，原生、轻量、启动快。

## 功能

- **浏览与解压**：树形浏览归档内容，支持解压全部 / 解压选中 / 单文件预览 / 用默认程序打开。
- **创建与编辑**：压缩文件或文件夹，向已有归档增删条目，可选压缩等级、排除系统垃圾文件（`.DS_Store`、`__MACOSX` 等）。
- **广泛的格式支持**：
  - 读写：`zip`（含 AES）、`7z`（含 AES-256）、`tar`、`gz`/`tgz`、`bz2`/`tbz2`、`xz`/`txz`、`zst`/`tzst`、`jar`、`apk`
  - 只读：`rar`
- **加密归档**：自动识别加密项并按需提示输入密码。
- **密码管理器**：保存常用归档密码；查看明文前需通过**系统认证**（macOS Touch ID / 登录密码，Windows Hello）。
- **文件关联管理**：一键把 Origami 设为各压缩格式的默认打开程序，并显示每种格式的当前处理器。
- **右键菜单集成**：
  - macOS：通过文件关联接入「打开方式」。
  - Windows：应用内一键安装/移除经典右键菜单（注册表，无需打包签名）。
- **编码识别**：对非 UTF-8 文件名自动探测编码（GBK 等），也可手动指定。
- **快捷压缩**：从系统右键菜单直接压缩时弹出迷你进度窗口，无需打开主界面。
- **可定制界面**：主题、字号、缩放等设置。
- **快捷键**：`Cmd/Ctrl + ,` 打开设置；`Cmd/Ctrl + +/-` 调整界面字号，`Cmd/Ctrl + 0` 复位。修饰键按平台自适应（macOS 用 Cmd，Windows/Linux 用 Ctrl）。

## 技术栈

| 层 | 技术 |
| --- | --- |
| 桌面框架 | Tauri 2 |
| 后端 | Rust（`zip`、`sevenz-rust2`、`unrar`、`tar`、`flate2`、`bzip2`、`liblzma`、`zstd`） |
| 前端 | React 18 + TypeScript + Vite 6 |
| 系统认证 | macOS LocalAuthentication（objc2）/ Windows Hello（`windows` crate） |

## 开发

前置：Node.js、Rust 工具链、Tauri 2 系统依赖。

```bash
npm install            # 安装前端依赖
npm run tauri dev      # 启动开发模式（Vite + Rust 调试二进制）
npm run tauri build    # 构建发行版 .app / .dmg（或对应平台产物）
```

仅校验，不打包：

```bash
npx tsc --noEmit                         # 前端类型检查
cd src-tauri && cargo check              # 后端编译检查
```

## 项目结构

```
src/                      前端（React/TS）
  App.tsx                 应用外壳、标题栏、归档打开/拖拽
  components/
    Browser.tsx           归档内容浏览器（列表、右键菜单、快捷键、属性）
    dialogs.tsx           设置、密码管理器、文件关联、属性等弹窗
    Welcome.tsx           欢迎页 / 最近打开
    MiniProgress.tsx      快捷压缩的迷你进度窗口
  api.ts                  对后端命令的封装
  settings.ts             本地设置（localStorage）

src-tauri/src/            后端（Rust）
  archive/                list / extract / create / edit / preview
  passwords.rs            密码存储（app_data_dir/passwords.json）
  sysauth.rs              系统级身份验证（Touch ID / Windows Hello）
  macassoc.rs             macOS 文件关联（LaunchServices）
  winassoc.rs             Windows 文件关联（注册表）
  winmenu.rs              Windows 经典右键菜单（注册表）
  sysicon.rs              系统文件图标
  encoding.rs             文件名编码探测

windows-extension/        Win11 新版顶层右键菜单脚手架（IExplorerCommand，需签名打包）
```

## 平台说明

- **密码可见性**：密码管理器在展示明文前调用系统认证；无可用认证的平台直接放行（不阻断用户查看自己的密码）。
- **Windows 右键菜单**：经典菜单（注册表）已接入主程序；Win11 新版顶层菜单需 MSIX/稀疏包签名注册，详见 [`windows-extension/README.md`](windows-extension/README.md)。
