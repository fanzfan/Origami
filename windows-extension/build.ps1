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
  [string] $PackMsixTo
)

$ErrorActionPreference = "Stop"
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
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

# Assets：稀疏包要求存在 Logo 资源。没有就用 Origami 的图标占位。
$assets = "$AppDir/Assets"
New-Item -ItemType Directory -Force -Path $assets | Out-Null
foreach ($n in @("StoreLogo.png","Square150x150Logo.png","Square44x44Logo.png")) {
  if (-not (Test-Path "$assets/$n")) {
    if (Test-Path "$AppDir/Origami.exe") {
      # 占位：复制任一已有 png；正式分发请放真实图标。
      $src = Get-ChildItem "$AppDir" -Filter *.png -Recurse -ErrorAction SilentlyContinue | Select-Object -First 1
      if ($src) { Copy-Item $src.FullName "$assets/$n" -Force }
    }
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

# 可选：打成一个签名的 .msix 文件（稀疏包，含清单 + DLL + Assets，exe 为外部内容）。
if ($PackMsixTo) {
  # 从 Windows SDK 找 makeappx / signtool（取版本号最高的一份）。
  function Find-SdkTool($name) {
    Get-ChildItem "C:/Program Files (x86)/Windows Kits/10/bin" -Filter $name -Recurse -ErrorAction SilentlyContinue |
      Sort-Object FullName -Descending | Select-Object -First 1 -ExpandProperty FullName
  }
  $makeappx = Find-SdkTool "makeappx.exe"
  $signtool = Find-SdkTool "signtool.exe"
  if (-not $makeappx -or -not $signtool) { throw "未找到 makeappx/signtool，请安装 Windows SDK。" }

  # 只打包扩展负载（清单 + DLL + Assets），不含 Origami.exe（它是外部内容）。
  $stage = Join-Path $env:TEMP "origami-msix-stage"
  Remove-Item $stage -Recurse -Force -ErrorAction SilentlyContinue
  New-Item -ItemType Directory -Force -Path "$stage/Assets" | Out-Null
  Copy-Item "$here/AppxManifest.xml" "$stage/AppxManifest.xml" -Force
  Copy-Item $dll "$stage/origami_explorer_command.dll" -Force
  Copy-Item "$AppDir/Assets/*" "$stage/Assets/" -Force

  New-Item -ItemType Directory -Force -Path (Split-Path $PackMsixTo) | Out-Null
  Write-Host "› 打包 .msix：$PackMsixTo"
  & $makeappx pack /d $stage /p $PackMsixTo /o /nv
  if ($LASTEXITCODE -ne 0) { throw "makeappx 打包失败。" }
  Write-Host "› 用 $CertSubject 签名 .msix…"
  & $signtool sign /fd SHA256 /sha1 $cert.Thumbprint /t http://timestamp.digicert.com $PackMsixTo
  if ($LASTEXITCODE -ne 0) { throw "signtool 签名失败。" }
  Remove-Item $stage -Recurse -Force -ErrorAction SilentlyContinue
  Write-Host "✓ 已生成签名 .msix：$PackMsixTo"
}

# 以「外部位置」方式注册稀疏包（开发模式，无需打成 .msix）。
# 需要先开启开发者模式：设置 › 隐私和安全性 › 开发者选项 › 开发人员模式。
Write-Host "› 注册稀疏包（ExternalLocation = $AppDir）…"
Add-AppxPackage -Register "$AppDir/AppxManifest.xml" -ExternalLocation $AppDir

Write-Host ""
Write-Host "✓ 完成。重启资源管理器使其生效："
Write-Host "    Stop-Process -Name explorer -Force; Start-Process explorer"
Write-Host "卸载：Get-AppxPackage *Origami.ShellExtension* | Remove-AppxPackage"
