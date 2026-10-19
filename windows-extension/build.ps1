# Origami Windows 右键菜单（IExplorerCommand 稀疏包）构建与开发注册脚本。
#
# 前置：Windows 10/11、Rust(msvc 工具链)、Windows SDK（提供 makeappx/signtool）。
# 用法（在已安装好的 Origami 上启用顶层新菜单）：
#   1. 先安装 Origami（让 Origami.exe 落在某目录，例如
#      "C:\Program Files\Origami\Origami.exe"）。
#   2. 以管理员或普通用户 PowerShell 运行：
#        ./build.ps1 -AppDir "C:\Program Files\Origami"
#   3. 重启资源管理器或注销重登。右键文件即见「用 Origami 压缩 ▸」。
#
# 这是开发/自用流程：用本机自签名证书签名并 sideload。分发给他人需用
# 受信任的 Authenticode 证书重新签名（与 macOS 顶层 Finder 扩展需公证同理）。

param(
  [Parameter(Mandatory = $true)] [string] $AppDir,
  [string] $CertSubject = "CN=Origami Local Signing",
  # 额外产出一个「签名的 .msix 文件」到此路径（稀疏包，便于归档/分发）。
  # 注意：稀疏包仍需以 Add-AppxPackage -ExternalLocation 安装，且证书要被信任，
  # 不能双击直接装；本仓库的开发安装走下面的 -Register 流程。
  [string] $PackMsixTo,
  # 只构建/打包，不向系统注册（一键构建脚本用，避免改动系统、免 Dev Mode/管理员）。
  [switch] $SkipRegister
)

$ErrorActionPreference = "Stop"
$here = Split-Path -Parent $MyInvocation.MyCommand.Path

# 带包标识的 COM 代理进程看不到 %LOCALAPPDATA% 下的普通目录（AppData 虚拟化；
# Programs 子目录除外），外部位置放在那里会导致激活报 0x80070003、菜单不出现。
# Tauri NSIS 默认装到 %LOCALAPPDATA%\Origami，正好踩中，需改装到 Programs 下。
$appDirFull = [IO.Path]::GetFullPath($AppDir).TrimEnd('\')
$localApp = [IO.Path]::GetFullPath($env:LOCALAPPDATA).TrimEnd('\')
if ($appDirFull.StartsWith("$localApp\", "OrdinalIgnoreCase") -and
    -not $appDirFull.StartsWith("$localApp\Programs\", "OrdinalIgnoreCase")) {
  throw "AppDir 不能位于 %LOCALAPPDATA% 下（Programs 子目录除外）：$AppDir`n" +
    "请把 Origami 装到例如 %LOCALAPPDATA%\Programs\Origami（NSIS 静默安装可加 /D=<目录>）。"
}
$target = "x86_64-pc-windows-msvc"

Write-Host "› 编译 IExplorerCommand DLL…"
Push-Location "$here/explorer-command"
cargo build --release --target $target
Pop-Location
$dll = "$here/explorer-command/target/$target/release/origami_explorer_command.dll"
if (-not (Test-Path $dll)) { throw "未找到 DLL：$dll" }

# DLL 必须与 Origami.exe 同目录（清单按相对路径解析，且需读取同目录的 exe）。
Write-Host "› 复制 DLL 到应用目录：$AppDir"
Copy-Item $dll "$AppDir/origami_explorer_command.dll" -Force
Copy-Item "$here/AppxManifest.xml" "$AppDir/AppxManifest.xml" -Force

# Assets：稀疏包要求存在 Logo 资源。优先用仓库内的真实图标（src-tauri/icons），
# 找不到再退化为复制 AppDir 里任一 png 占位。
$assets = "$AppDir/Assets"
New-Item -ItemType Directory -Force -Path $assets | Out-Null
$repoIcons = Resolve-Path "$here/../src-tauri/icons" -ErrorAction SilentlyContinue
foreach ($n in @("StoreLogo.png","Square150x150Logo.png","Square44x44Logo.png")) {
  if (Test-Path "$assets/$n") { continue }
  if ($repoIcons -and (Test-Path "$repoIcons/$n")) {
    Copy-Item "$repoIcons/$n" "$assets/$n" -Force
  } else {
    # 占位：复制任一已有 png；正式分发请放真实图标。
    $src = Get-ChildItem "$AppDir" -Filter *.png -Recurse -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($src) { Copy-Item $src.FullName "$assets/$n" -Force }
  }
}

# 本机自签名证书（用于 sideload 信任）。
$cert = Get-ChildItem Cert:\CurrentUser\My | Where-Object { $_.Subject -eq $CertSubject } | Select-Object -First 1
if (-not $cert) {
  Write-Host "› 创建自签名证书：$CertSubject"
  $cert = New-SelfSignedCertificate -Type Custom -Subject $CertSubject `
    -KeyUsage DigitalSignature -FriendlyName "Origami Local Signing" `
    -CertStoreLocation "Cert:\CurrentUser\My" `
    -TextExtension @("2.5.29.37={text}1.3.6.1.5.5.7.3.3", "2.5.29.19={text}")
  # 信任：导入到本机 Trusted People，使 sideload 包可被接受。
  $tmp = "$env:TEMP/origami-cert.cer"
  Export-Certificate -Cert $cert -FilePath $tmp | Out-Null
  Import-Certificate -FilePath $tmp -CertStoreLocation "Cert:\LocalMachine\TrustedPeople" | Out-Null
  Remove-Item $tmp -Force
}

# 打成签名的稀疏 .msix：包内只有清单 + Assets；DLL 与 Origami.exe 都是外部内容，
# 留在 AppDir。清单里的 DllPath / Executable 相对 ExternalLocation 解析，DLL 再按
# 自身所在目录找 Origami.exe —— 若把 DLL 也打进包，它会被装进 WindowsApps，
# 从那里找不到 Origami.exe。
# 注册必须走这个 .msix：松散清单 `Add-AppxPackage -Register -ExternalLocation`
# 会报 0x80073CF0（找不到 AppxManifest.xml）。
if (-not $PackMsixTo -and -not $SkipRegister) {
  $PackMsixTo = Join-Path $env:TEMP "Origami.ShellExtension.msix"
}
if ($PackMsixTo) {
  # 从 Windows SDK 找 makeappx / signtool（取版本号最高的一份）。
  function Find-SdkTool($name) {
    Get-ChildItem "C:/Program Files (x86)/Windows Kits/10/bin" -Filter $name -Recurse -ErrorAction SilentlyContinue |
      Sort-Object FullName -Descending | Select-Object -First 1 -ExpandProperty FullName
  }
  $makeappx = Find-SdkTool "makeappx.exe"
  $signtool = Find-SdkTool "signtool.exe"
  if (-not $makeappx -or -not $signtool) { throw "未找到 makeappx/signtool，请安装 Windows SDK。" }

  $stage = Join-Path $env:TEMP "origami-msix-stage"
  Remove-Item $stage -Recurse -Force -ErrorAction SilentlyContinue
  New-Item -ItemType Directory -Force -Path "$stage/Assets" | Out-Null
  Copy-Item "$here/AppxManifest.xml" "$stage/AppxManifest.xml" -Force
  Copy-Item "$AppDir/Assets/*" "$stage/Assets/" -Force

  New-Item -ItemType Directory -Force -Path (Split-Path $PackMsixTo) | Out-Null
  Write-Host "› 打包 .msix：$PackMsixTo"
  & $makeappx pack /d $stage /p $PackMsixTo /o /nv
  if ($LASTEXITCODE -ne 0) { throw "makeappx 打包失败。" }
  Write-Host "› 用 $CertSubject 签名 .msix…"
  & $signtool sign /fd SHA256 /sha1 $cert.Thumbprint $PackMsixTo
  if ($LASTEXITCODE -ne 0) { throw "signtool 签名失败。" }
  Remove-Item $stage -Recurse -Force -ErrorAction SilentlyContinue
  Write-Host "✓ 已生成签名 .msix：$PackMsixTo"
}

if ($SkipRegister) {
  Write-Host ""
  Write-Host "✓ 已跳过注册（-SkipRegister）。DLL/清单/Assets 已就位于：$AppDir"
  return
}

# 以「外部位置」方式安装稀疏包。需要开发者模式（设置 › 隐私和安全性 › 开发者选项）。
# 同版本号的包不能覆盖安装，先卸掉旧的。
Get-AppxPackage Origami.ShellExtension | Remove-AppxPackage
Write-Host "› 注册稀疏包（ExternalLocation = $AppDir）…"
Add-AppxPackage -Path $PackMsixTo -ExternalLocation $AppDir

Write-Host ""
Write-Host "✓ 完成。重启资源管理器使其生效："
Write-Host "    Stop-Process -Name explorer -Force; Start-Process explorer"
Write-Host "卸载：Get-AppxPackage *Origami.ShellExtension* | Remove-AppxPackage"
