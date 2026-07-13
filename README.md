# Origami

Origami 是一款基于 **Tauri 2 + Rust + React/TypeScript** 的跨平台压缩包管理器，目标是提供接近 Bandizip / 7-Zip 的轻量日常体验。

## 功能

- **浏览与解压**：在应用内浏览文件系统和归档目录树，支持全部/选中解压、预览、默认程序打开和完整性校验。
- **创建与编辑**：压缩文件或文件夹，向现有归档添加或删除条目，可设置压缩等级、密码、分卷和系统垃圾文件过滤。
- **归档格式**：
  - 读写：`zip`（含 AES）、`7z`（含 AES-256）、`tar`、`gz`/`tgz`、`bz2`/`tbz2`、`xz`/`txz`、`zst`/`tzst`、`jar`、`apk`
  - 只读：`rar`
- **编码处理**：自动探测非 UTF-8 文件名编码，也可手动选择 GBK、Big5、Shift-JIS 等编码。
- **密码保护**：明文密码保存到 macOS 钥匙串或 Windows 凭据管理器，本地 `passwords.json` 只保存索引；查看明文前使用 Touch ID / Windows Hello 等系统认证。
- **系统集成**：管理文件关联；在 Finder / 资源管理器右键菜单中直接压缩或解压；快捷任务使用独立进度窗口。
- **界面设置**：支持深浅模式、主题、字体、缩放和窗口材质。
- **国际化基础**：语言可跟随系统，也可选择简体中文或 English；欢迎页、应用外壳、任务状态和设置页已接入 i18n，其余界面按相同资源结构继续迁移。

## 技术栈

| 层 | 技术 |
| --- | --- |
| 桌面框架 | Tauri 2 |
| 后端 | Rust（`zip`、`sevenz-rust2`、`unrar`、`tar`、`flate2`、`bzip2`、`liblzma`、`zstd`） |
| 前端 | React 18 + TypeScript + Vite 6 |
| 国际化 | i18next + react-i18next |
| 系统认证 | macOS LocalAuthentication / Windows Hello |

## 开发

需要 Node.js、Rust 工具链和当前平台对应的 Tauri 2 系统依赖。

```bash
npm install
npm run tauri dev
npm run tauri build
```

提交改动前至少运行：

```bash
npx tsc --noEmit
cd src-tauri && cargo check
```

当前平台无法覆盖另一平台的 `#[cfg(...)]` 分支；涉及系统集成时，还需要在目标系统上做运行验证。Windows 需要 MSVC Rust 工具链、WebView2 Runtime 和包含 C++ 工作负载的 Visual Studio Build Tools。

## 项目结构

```text
src/
  App.tsx                 主窗口、归档任务和全局交互
  main.tsx                主窗口 / 快捷进度窗 / 交互任务窗入口
  i18n/                   语言解析与翻译资源
  components/
    Welcome.tsx           欢迎页与最近打开
    FileExplorer.tsx      文件系统浏览
    Browser.tsx           归档内容浏览与编辑
    dialogs.tsx           设置、压缩、解压、密码等弹窗
    MiniProgress.tsx      快捷任务进度窗口
    AskDialog.tsx         需要用户配置的快捷任务窗口
  api.ts                  Tauri 命令封装
  settings.ts             持久化设置

src-tauri/src/
  archive/                归档识别、列出、解压、创建、编辑和预览
  cli.rs                  命令行与深链动作解析
  services.rs             macOS Finder Quick Action
  winmenu.rs              Windows 经典右键菜单
  macassoc.rs/winassoc.rs 文件关联
  sysauth.rs              Touch ID / Windows Hello
  passwords.rs            系统凭据库与本地索引
  sysicon.rs              系统文件图标

windows-extension/        Win11 新版顶层菜单（IExplorerCommand + 稀疏包）
finder-extension/         macOS Finder 扩展相关代码
```

## 国际化开发

- 翻译资源位于 `src/i18n/locales/`，按功能分组使用稳定的语义键，不把中文原文当作 key。
- 新增用户可见文案时同时补齐 `zh-CN.ts` 与 `en-US.ts`；品牌名、文件路径、格式名和后端错误原文无需翻译。
- 组件内使用 `useTranslation()`；非 React 代码可导入 `i18n` 实例。含数量或变量的文案通过插值生成，不在 JSX 中拼接句子。
- `settings.language` 保存 `system`、`zh-CN` 或 `en-US`。`system` 会解析浏览器首选语言，暂不支持的语言回退到 English。
- 完成一批迁移后，应分别切换中英文检查布局、按钮宽度、空状态和任务弹窗。

## 平台集成

- **macOS**：Finder Quick Action 可安装/移除压缩与解压动作。由于 LaunchServices 可能把同 bundle id 的已安装版本带到前台，本地 UI 验证以最新打包并安装的 `.app` 为准。
- **Windows**：应用内可安装/移除基于 HKCU 的经典右键菜单；Win11 新版顶层菜单需要签名和稀疏包注册，见 [`windows-extension/README.md`](windows-extension/README.md)。
- **密码存储**：旧版明文索引会在读取或写入入口迁移到系统凭据库；迁移成功后本地文件不再保留明文。
