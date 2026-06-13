# Origami Windows 右键菜单

Windows 上有两条右键菜单路径，对应两种菜单形态：

| 形态 | 实现 | 状态 | 备注 |
| --- | --- | --- | --- |
| **经典菜单**（Win11「显示更多选项 / Shift+F10」；Win10 直接顶层） | 注册表 `HKCU\Software\Classes`（`src-tauri/src/winmenu.rs`） | **已接入主程序** | 应用内「🧩 右键菜单集成」一键安装/移除，无需签名打包 |
| **Win11 新版顶层菜单** | 本目录的 IExplorerCommand COM 处理器 + MSIX 稀疏包 | **休眠脚手架** | 必须签名打包注册，和 macOS 顶层 Finder 扩展的限制一致 |

经典菜单已经够日常使用；本目录是想进一步进入 Win11「新版顶层菜单」时的实现。

## 为什么顶层新菜单需要打包

Win11 的新版右键菜单**只**接受打包应用（MSIX/稀疏包）提供的 `IExplorerCommand`
处理器，且包必须经过签名、证书被本机信任。这与 macOS 顶层 Finder 扩展必须
Developer ID + 公证是同一类平台限制。未打包的 Tauri 应用只能写经典注册表菜单。

## 目录内容

- `explorer-command/` — 进程内 COM 服务器（cdylib），实现顶层项及三个子项；
  子项 Invoke 时启动同目录 `Origami.exe --compress=<zip|7z|ask> "<路径>"`。
- `AppxManifest.xml` — 稀疏包清单：声明 COM 服务器 + 挂到
  `windows.fileExplorerContextMenus`。
- `build.ps1` — 在 Windows 上编译 DLL、自签名、以 ExternalLocation 方式注册（开发用）。

## 开发启用步骤（Windows）

1. 安装 Origami，记下安装目录（含 `Origami.exe`）。
2. 打开「开发人员模式」（设置 › 隐私和安全性 › 开发者选项）。
3. 运行：
   ```powershell
   cd windows-extension
   ./build.ps1 -AppDir "C:\Program Files\Origami"
   ```
4. 重启资源管理器：`Stop-Process -Name explorer -Force; Start-Process explorer`
5. 右键文件/文件夹 → 顶层出现「用 Origami 压缩 ▸」。

卸载：`Get-AppxPackage *Origami.ShellExtension* | Remove-AppxPackage`

## 注意

- COM 代码按 `windows` crate 0.58 接口编写，**已在 Windows 上 `cargo build --release` 编译通过**
  （需 `windows` 的 `implement` 特性；0.58 里 `IExplorerCommand` 等方法的 shell 项参数为
  `Option<&IShellItemArray>`、`IClassFactory::CreateInstance` 的 outer 为 `Option<&IUnknown>`、
  `IEnumExplorerCommand::Skip/Reset` 返回 `Result<()>`）。换 crate 版本时这些签名可能再次需要微调。
  运行期行为（顶层菜单真正出现并能拉起 Origami.exe）仍需在设备上注册后验证。
- `AppxManifest.xml` 的 `<Identity Publisher=...>` 必须与签名证书 Subject 完全一致。
- `CLSID` 在 `explorer-command/src/lib.rs` 与 `AppxManifest.xml` 两处必须一致。
- 分发给他人需用受信任的 Authenticode 证书重新签名，不能用自签名。
