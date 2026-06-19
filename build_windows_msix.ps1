<#
.SYNOPSIS
  Origami Windows 一键构建：检查环境 → 构建主程序(Origami.exe + NSIS) → 编译右键
  菜单 DLL → 打包并签名稀疏 MSIX(.msix)。默认只构建、不向系统注册。

.DESCRIPTION
  开跑前会逐项检查构建所需环境，任一硬性条件不满足即终止并给出修复提示：
    - Windows 操作系统
    - Node.js / npm
    - Rust(cargo) 且 host 为 *-pc-windows-msvc
    - VS C++ 生成工具(VC Tools，用 vswhere 探测)
    - Windows SDK 的 makeappx.exe / signtool.exe
  另外软性检查 WebView2 运行时(仅运行需要)，-Register 时检查开发者模式。

.PARAMETER OutDir
  产物输出目录。默认 src-tauri/target/release/bundle。

.PARAMETER CertSubject
  签名用自签名证书的 Subject。默认与 AppxManifest 的 Publisher 一致。

.PARAMETER SkipAppBuild
  跳过 npm run tauri build（已构建过 Origami.exe 时用，省时间）。

.PARAMETER Register
  构建后顺带把稀疏包注册到系统（需开发者模式 + 管理员）。注册的 ExternalLocation
  指向输出的 AppDir，该目录被移动/删除后扩展会失效。

.EXAMPLE
  pwsh -File build_windows_msix.ps1
  pwsh -File build_windows_msix.ps1 -SkipAppBuild
  pwsh -File build_windows_msix.ps1 -Register
#>
[CmdletBinding()]
param(
  [string] $OutDir,
  [string] $CertSubject = "CN=Origami Local Signing",
  [switch] $SkipAppBuild,
  [switch] $Register
)

$ErrorActionPreference = "Stop"
$repo = $PSScriptRoot
if (-not $OutDir) { $OutDir = Join-Path $repo "src-tauri/target/release/bundle" }

function Write-Step($msg) { Write-Host "`n=== $msg ===" -ForegroundColor Cyan }
function Ok($m)   { Write-Host "  [✓] $m" -ForegroundColor Green }
function Warn($m) { Write-Host "  [!] $m" -ForegroundColor Yellow }
function Bad($m)  { Write-Host "  [✗] $m" -ForegroundColor Red }

# ---------------------------------------------------------------------------
# 1) 环境检查
# ---------------------------------------------------------------------------
Write-Step "检查构建环境"
$fail = @()

# 操作系统
if ([System.Runtime.InteropServices.RuntimeInformation]::IsOSPlatform(
      [System.Runtime.InteropServices.OSPlatform]::Windows)) {
  Ok "操作系统：Windows"
} else {
  Bad "非 Windows 系统，无法构建 MSIX"; $fail += "OS"
}

# Node / npm
$node = Get-Command node -ErrorAction SilentlyContinue
$npm  = Get-Command npm  -ErrorAction SilentlyContinue
if ($node) { Ok "Node.js：$(& node --version)" } else { Bad "未找到 node，请安装 Node.js LTS"; $fail += "node" }
if ($npm)  { Ok "npm：$(& npm --version)" }       else { Bad "未找到 npm"; $fail += "npm" }

# Rust / cargo + msvc host
$cargo = Get-Command cargo -ErrorAction SilentlyContinue
if ($cargo) {
  $host_ = (& rustc -vV | Select-String '^host:').ToString().Split(':')[1].Trim()
  if ($host_ -like "*pc-windows-msvc*") {
    Ok "Rust：$(& rustc --version)（host $host_）"
  } else {
    Bad "Rust host 是 $host_，需 *-pc-windows-msvc 工具链(rustup default stable-msvc)"; $fail += "msvc-host"
  }
} else {
  Bad "未找到 cargo，请安装 Rust(rustup)"; $fail += "cargo"
}

# VS C++ 生成工具（VC Tools）
$vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
if (Test-Path $vswhere) {
  $vc = & $vswhere -latest -products * `
    -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 `
    -property installationPath 2>$null
  if ($vc) { Ok "VS C++ 生成工具：$vc" }
  else { Bad "未装 VC++ 工作负载(MSVC 链接器)，装 VS Build Tools 的「使用 C++ 的桌面开发」"; $fail += "vctools" }
} else {
  Warn "未找到 vswhere，无法确认 VC++ 工具；若后续链接失败请安装 VS Build Tools"
}

# Windows SDK：makeappx / signtool
function Find-SdkTool($name) {
  Get-ChildItem "C:/Program Files (x86)/Windows Kits/10/bin" -Filter $name -Recurse -ErrorAction SilentlyContinue |
    Sort-Object FullName -Descending | Select-Object -First 1 -ExpandProperty FullName
}
$makeappx = Find-SdkTool "makeappx.exe"
$signtool = Find-SdkTool "signtool.exe"
if ($makeappx) { Ok "makeappx：$makeappx" } else { Bad "未找到 makeappx.exe，请安装 Windows SDK"; $fail += "makeappx" }
if ($signtool) { Ok "signtool：$signtool" } else { Bad "未找到 signtool.exe，请安装 Windows SDK"; $fail += "signtool" }

# WebView2 运行时（软性，仅运行需要）
$wv2Keys = @(
  "HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}",
  "HKLM:\SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}",
  "HKCU:\SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}"
)
$wv2 = $wv2Keys | ForEach-Object { (Get-ItemProperty $_ -Name pv -ErrorAction SilentlyContinue).pv } |
  Where-Object { $_ } | Select-Object -First 1
if ($wv2) { Ok "WebView2 运行时：$wv2" } else { Warn "未检出 WebView2 运行时（构建不需要，运行 Origami 需要）" }

# 开发者模式（仅 -Register 时需要）
if ($Register) {
  $devmode = (Get-ItemProperty "HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\AppModelUnlock" `
    -Name AllowDevelopmentWithoutDevLicense -ErrorAction SilentlyContinue).AllowDevelopmentWithoutDevLicense
  if ($devmode -eq 1) { Ok "开发者模式：已开启" }
  else { Bad "未开启开发者模式，-Register 需要它（设置→隐私和安全性→开发者选项）"; $fail += "devmode" }
}

if ($fail.Count -gt 0) {
  Write-Host "`n环境检查未通过：$($fail -join ', ')。请按上面提示修复后重试。" -ForegroundColor Red
  exit 1
}
Ok "环境检查全部通过"

# ---------------------------------------------------------------------------
# 2) 依赖 + 构建主程序
# ---------------------------------------------------------------------------
Push-Location $repo
try {
  if (-not (Test-Path "$repo/node_modules")) {
    Write-Step "安装前端依赖(npm install)"
    & npm install
    if ($LASTEXITCODE -ne 0) { throw "npm install 失败。" }
  }

  if ($SkipAppBuild) {
    Write-Step "跳过主程序构建(-SkipAppBuild)"
  } else {
    Write-Step "构建主程序(npm run tauri build：Origami.exe + NSIS)"
    & npm run tauri build
    if ($LASTEXITCODE -ne 0) { throw "tauri build 失败。" }
  }

  $exe = Join-Path $repo "src-tauri/target/release/Origami.exe"
  if (-not (Test-Path $exe)) { throw "未找到 $exe（先不要 -SkipAppBuild，或检查构建输出）。" }
  Ok "主程序：$exe"
  $nsis = Get-ChildItem "$repo/src-tauri/target/release/bundle/nsis" -Filter "*-setup.exe" -ErrorAction SilentlyContinue |
    Select-Object -First 1
  if ($nsis) { Ok "NSIS 安装包：$($nsis.FullName)" }

  # ---------------------------------------------------------------------------
  # 3) 组装 AppDir + 打包/签名 .msix（复用 windows-extension/build.ps1）
  # ---------------------------------------------------------------------------
  Write-Step "打包并签名 MSIX 稀疏包"
  $appDir = Join-Path $OutDir "msix-appdir"
  New-Item -ItemType Directory -Force -Path $appDir | Out-Null
  Copy-Item $exe (Join-Path $appDir "Origami.exe") -Force   # 作为 ExternalLocation 的外部内容
  $msixOut = Join-Path $OutDir "Origami.ShellExtension.msix"

  $buildPs1 = Join-Path $repo "windows-extension/build.ps1"
  $args = @{ AppDir = $appDir; CertSubject = $CertSubject; PackMsixTo = $msixOut }
  if (-not $Register) { $args.SkipRegister = $true }
  & $buildPs1 @args
}
finally {
  Pop-Location
}

# ---------------------------------------------------------------------------
# 4) 收尾
# ---------------------------------------------------------------------------
Write-Step "完成"
Ok "签名 MSIX：$msixOut"
Ok "右键菜单就绪的 AppDir：$appDir"
if ($nsis) { Ok "NSIS 安装包：$($nsis.FullName)" }
if (-not $Register) {
  Write-Host ""
  Write-Host "提示：本脚本只构建未注册。要启用 Win11 顶层右键菜单，安装后执行：" -ForegroundColor DarkGray
  Write-Host "  windows-extension/build.ps1 -AppDir `"<Origami 安装目录>`"" -ForegroundColor DarkGray
  Write-Host "（需开发者模式 + 管理员；或本次加 -Register 直接注册到上面的 AppDir）" -ForegroundColor DarkGray
}
