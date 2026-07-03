# macOS 待验证 / 待补事项（本轮 Windows 侧工作的对照清单）

本轮改动主要在 Windows 上开发与实测（右键菜单解压、菜单扁平化、tar.\* 识别增强）。
下列涉及 macOS 的部分**尚未在 mac 上编译/运行验证**，请在 macOS 机器上按此清单核对。

校验命令：

```bash
npx tsc --noEmit                  # 前端类型检查
cd src-tauri && cargo check       # 后端编译检查（darwin 目标）
cargo test --lib archive::        # 归档识别/列出单测（跨平台，应全绿）
npm run tauri build               # 出 .app / .dmg
```

---

## 1. Finder 右键「解压」Quick Action（services.rs，仅 macOS 编译）

`src-tauri/src/services.rs` 已从「只压缩」扩展为「压缩 + 解压」，新增三条解压服务，
通过 `origami://extract?mode=here|folder|ask` 深链唤起主程序。**这段是 `#[cfg(target_os="macos")]`，
在 Windows 上没被 `cargo check` 覆盖，务必在 mac 上确认能编译。**

需要在 macOS 实测：

- [ ] `cargo check` / `cargo build` 通过（services.rs 的 `ServiceDef { url_base, param, send_types }`
      重构、`info_plist(menu_title, send_types)`、`document_wflow(&ServiceDef)`、`uuid_pad(param)` 等签名改动）。
- [ ] 打包版 Origami.app 跑一次以注册 `origami://` scheme。
- [ ] 应用内「🧩 右键菜单集成」对话框 → 安装 → 重启 Finder。
- [ ] 右键**压缩包**（.zip/.tar.gz/.tar.zst 等）出现三项解压：
      `解压到当前文件夹` / `解压到单独文件夹` / `解压到…（选择位置）`，行为正确。
- [ ] 解压服务仅对归档 UTI 显示（`ARCHIVE_TYPES`：zip/tar/gzip/bzip2/rar/7z/archive）。
      注意：缺少系统 UTI 的格式（如 .7z、.zst、.tar.zst）可能**不出现** Quick Action —— 这是
      已知取舍（macOS 按 UTI 过滤，不像 Windows 能按扩展名挂）。若希望覆盖更多，需要在
      `NSSendFileTypes` 里补 UTI，或改成 `public.data` 放宽后在脚本里自行过滤。
- [ ] 「解压后打开目标文件夹」设置：解压完成后应打开解压目录（受设置开关控制）。
- [ ] 移除服务后 Finder 菜单干净消失。

## 2. 解压后打开目标目录（前端，跨平台，mac 侧确认交互）

- [ ] `src/settings.ts` 新增 `openAfterExtract`（默认 true）；设置面板「压缩与解压」有对应开关。
- [ ] 关掉开关后，应用内解压与快捷解压都不再自动打开目录；打开时用 `openPath(out)`（打开目录本身，
      而非 `revealItemInDir`）。确认 macOS 上 `openPath` 对目录的行为符合预期（用访达打开该目录）。

## 3. tar.\* 识别增强（archive/mod.rs，跨平台）

`detect_format` 增加了「内容嗅探」：单层压缩（.gz/.bz2/.xz/.zst）若解压开头是 tar（偏移 257 的
`ustar` 魔数），自动升级为 `TAR.GZ`/`TAR.ZST` 等；无扩展名的裸 tar 也能按魔数识别。

- [ ] `cargo test --lib archive::` 全绿（新增 5 个用例，跨平台）。
- [ ] macOS 上打开命名不规范的 `foo.gz`/`foo.zst`（其实是 tarball）能展开目录树。
- [ ] 真单文件 gzip（如 `notes.txt.gz`）仍是单文件视图，未被误判。

## 4. Windows 专属改动（mac 无需处理，仅备忘）

以下为 Windows 专有，macOS 不参与编译/运行，列出仅供了解本轮范围：

- `src-tauri/src/winmenu.rs`：经典右键菜单从级联子菜单**改为平铺顶层动词**（压缩 3 项 + 解压 3 项），
  `install` 会先清理旧级联键（`Origami` / `Origami.Extract`）以便平滑升级。
- `windows-extension/`（IExplorerCommand 稀疏包）：Win11 顶层新菜单也**平铺**为 6 个独立顶层命令
  （各自 CLSID），解压项通过 `GetState` 对非压缩包返回 `ECS_HIDDEN` 动态过滤。`AppxManifest.xml`
  升到 1.0.2.0。这套需要 Dev Mode + 自签名证书 sideload，与 mac 无关。
- `src-tauri/src/cli.rs` / `lib.rs`：`--extract=<here|folder|ask>` 解析、`PendingAction::Extract`、
  `quick_extract_dest`、`request_open_in_main` 等为跨平台，但快捷解压的迷你窗驱动主要在 Windows 冷启动路径上验证过，
  mac 上（RunEvent::Opened 深链）建议顺带回归一次右键「解压到当前文件夹」的完整链路。

## 5. 可选：更多 Linux/UNIX 归档格式（未做）

用户提到「等 Linux/UNIX 常用格式」。当前完整支持 zip/7z/rar/tar 及 tar.{gz,bz2,xz,zst} 与单层
gz/bz2/xz/zst。**未覆盖**：lz4 / lzip(.lz) / lzma(.lzma) / lzop(.lzo) / compress(.Z) / cpio / deb / rpm。
如需支持，各自要引入解码库并接入 `Format` / `detect_format` / `list` / `extract`，属于独立一档工作。
