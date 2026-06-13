//! Finder Quick Action (右键菜单) 安装与卸载。
//!
//! 在 ~/Library/Services 写入 Automator .workflow 包，工作流接收 Finder
//! 选中的文件路径，base64url 编码后通过 origami:// URL scheme 唤起本应用。

use anyhow::Context;
use std::path::PathBuf;

pub struct ServiceDef {
    pub menu_title: &'static str,
    pub format: &'static str,
}

pub const SERVICES: &[ServiceDef] = &[
    ServiceDef { menu_title: "用 Origami 压缩为 ZIP", format: "zip" },
    ServiceDef { menu_title: "用 Origami 压缩为 7Z", format: "7z" },
    ServiceDef { menu_title: "用 Origami 压缩（详细设置…）", format: "ask" },
];

fn services_dir() -> anyhow::Result<PathBuf> {
    let home = std::env::var("HOME").context("HOME 未设置")?;
    Ok(PathBuf::from(home).join("Library/Services"))
}

fn workflow_path(s: &ServiceDef) -> anyhow::Result<PathBuf> {
    Ok(services_dir()?.join(format!("{}.workflow", s.menu_title)))
}

fn shell_script(format: &str) -> String {
    format!(
        r#"url="origami://create?format={format}"
for f in "$@"; do
  b=$(printf %s "$f" | /usr/bin/base64 | /usr/bin/tr -d '\n' | /usr/bin/sed -e 's/+/-/g' -e 's,/,_,g' -e 's/=//g')
  url="$url&p=$b"
done
/usr/bin/open "$url"
"#
    )
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn info_plist(menu_title: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>NSServices</key>
  <array>
    <dict>
      <key>NSBackgroundColorName</key>
      <string>background</string>
      <key>NSIconName</key>
      <string>workflowCustomImage</string>
      <key>NSMenuItem</key>
      <dict>
        <key>default</key>
        <string>{title}</string>
      </dict>
      <key>NSMessage</key>
      <string>runWorkflowAsService</string>
      <key>NSRequiredContext</key>
      <dict>
        <key>NSApplicationIdentifier</key>
        <string>com.apple.finder</string>
      </dict>
      <key>NSSendFileTypes</key>
      <array>
        <string>public.item</string>
      </array>
    </dict>
  </array>
</dict>
</plist>
"#,
        title = xml_escape(menu_title)
    )
}

fn document_wflow(format: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>AMApplicationBuild</key>
  <string>521</string>
  <key>AMApplicationVersion</key>
  <string>2.10</string>
  <key>AMDocumentVersion</key>
  <string>2</string>
  <key>actions</key>
  <array>
    <dict>
      <key>action</key>
      <dict>
        <key>AMAccepts</key>
        <dict>
          <key>Container</key>
          <string>List</string>
          <key>Optional</key>
          <true/>
          <key>Types</key>
          <array>
            <string>com.apple.cocoa.string</string>
          </array>
        </dict>
        <key>AMActionVersion</key>
        <string>2.0.3</string>
        <key>AMApplication</key>
        <array>
          <string>Automator</string>
        </array>
        <key>AMParameterProperties</key>
        <dict>
          <key>COMMAND_STRING</key>
          <dict/>
          <key>CheckedForUserDefaultShell</key>
          <dict/>
          <key>inputMethod</key>
          <dict/>
          <key>shell</key>
          <dict/>
          <key>source</key>
          <dict/>
        </dict>
        <key>AMProvides</key>
        <dict>
          <key>Container</key>
          <string>List</string>
          <key>Types</key>
          <array>
            <string>com.apple.cocoa.string</string>
          </array>
        </dict>
        <key>ActionBundlePath</key>
        <string>/System/Library/Automator/Run Shell Script.action</string>
        <key>ActionName</key>
        <string>运行 Shell 脚本</string>
        <key>ActionParameters</key>
        <dict>
          <key>COMMAND_STRING</key>
          <string>{script}</string>
          <key>CheckedForUserDefaultShell</key>
          <true/>
          <key>inputMethod</key>
          <integer>1</integer>
          <key>shell</key>
          <string>/bin/zsh</string>
          <key>source</key>
          <string></string>
        </dict>
        <key>BundleIdentifier</key>
        <string>com.apple.RunShellScript</string>
        <key>CFBundleVersion</key>
        <string>2.0.3</string>
        <key>CanShowSelectedItemsWhenRun</key>
        <false/>
        <key>CanShowWhenRun</key>
        <true/>
        <key>Category</key>
        <array>
          <string>AMCategoryUtilities</string>
        </array>
        <key>Class Name</key>
        <string>RunShellScriptAction</string>
        <key>InputUUID</key>
        <string>9C2E4373-2C7A-4AE6-8DF5-{format_pad}</string>
        <key>Keywords</key>
        <array>
          <string>Shell</string>
        </array>
        <key>OutputUUID</key>
        <string>7B6F2E18-9D14-4F8A-AC02-{format_pad}</string>
        <key>UUID</key>
        <string>1A3B5C7D-0E2F-4A6B-8C0D-{format_pad}</string>
        <key>UnlocalizedApplications</key>
        <array>
          <string>Automator</string>
        </array>
        <key>arguments</key>
        <dict>
          <key>0</key>
          <dict>
            <key>default value</key>
            <integer>0</integer>
            <key>name</key>
            <string>inputMethod</string>
            <key>required</key>
            <string>0</string>
            <key>type</key>
            <string>0</string>
            <key>uuid</key>
            <string>0</string>
          </dict>
          <key>1</key>
          <dict>
            <key>default value</key>
            <false/>
            <key>name</key>
            <string>CheckedForUserDefaultShell</string>
            <key>required</key>
            <string>0</string>
            <key>type</key>
            <string>0</string>
            <key>uuid</key>
            <string>1</string>
          </dict>
          <key>2</key>
          <dict>
            <key>default value</key>
            <string></string>
            <key>name</key>
            <string>source</string>
            <key>required</key>
            <string>0</string>
            <key>type</key>
            <string>0</string>
            <key>uuid</key>
            <string>2</string>
          </dict>
          <key>3</key>
          <dict>
            <key>default value</key>
            <string></string>
            <key>name</key>
            <string>COMMAND_STRING</string>
            <key>required</key>
            <string>0</string>
            <key>type</key>
            <string>0</string>
            <key>uuid</key>
            <string>3</string>
          </dict>
          <key>4</key>
          <dict>
            <key>default value</key>
            <string>/bin/sh</string>
            <key>name</key>
            <string>shell</string>
            <key>required</key>
            <string>0</string>
            <key>type</key>
            <string>0</string>
            <key>uuid</key>
            <string>4</string>
          </dict>
        </dict>
        <key>isViewVisible</key>
        <integer>1</integer>
      </dict>
    </dict>
  </array>
  <key>connectors</key>
  <dict/>
  <key>workflowMetaData</key>
  <dict>
    <key>applicationBundleID</key>
    <string>com.apple.finder</string>
    <key>applicationBundleIDsByPath</key>
    <dict>
      <key>/System/Library/CoreServices/Finder.app</key>
      <string>com.apple.finder</string>
    </dict>
    <key>applicationPath</key>
    <string>/System/Library/CoreServices/Finder.app</string>
    <key>applicationPaths</key>
    <array>
      <string>/System/Library/CoreServices/Finder.app</string>
    </array>
    <key>customImageFileData</key>
    <data>{icon_b64}</data>
    <key>customImageFileExtension</key>
    <string>icns</string>
    <key>inputTypeIdentifier</key>
    <string>com.apple.Automator.fileSystemObject</string>
    <key>outputTypeIdentifier</key>
    <string>com.apple.Automator.nothing</string>
    <key>presentationMode</key>
    <integer>15</integer>
    <key>processesInput</key>
    <false/>
    <key>serviceApplicationBundleID</key>
    <string>com.apple.finder</string>
    <key>serviceApplicationPath</key>
    <string>/System/Library/CoreServices/Finder.app</string>
    <key>serviceInputTypeIdentifier</key>
    <string>com.apple.Automator.fileSystemObject</string>
    <key>serviceOutputTypeIdentifier</key>
    <string>com.apple.Automator.nothing</string>
    <key>serviceProcessesInput</key>
    <false/>
    <key>useAutomaticInputType</key>
    <false/>
    <key>workflowTypeIdentifier</key>
    <string>com.apple.Automator.servicesMenu</string>
  </dict>
</dict>
</plist>
"#,
        script = xml_escape(&shell_script(format)),
        format_pad = uuid_pad(format),
        icon_b64 = menu_icon_b64(),
    )
}

fn menu_icon_b64() -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .encode(include_bytes!("../icons/menu.icns"))
}

/// 用 format 名生成稳定的 12 位十六进制尾段，保证三个工作流 UUID 互不相同。
fn uuid_pad(format: &str) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in format.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{:012X}", h & 0xFFFF_FFFF_FFFF)
}

pub fn install() -> anyhow::Result<()> {
    for s in SERVICES {
        let wf = workflow_path(s)?;
        let contents = wf.join("Contents");
        std::fs::create_dir_all(&contents)?;
        std::fs::write(contents.join("Info.plist"), info_plist(s.menu_title))?;
        std::fs::write(contents.join("document.wflow"), document_wflow(s.format))?;
        // pbs 仅注册服务，不会自动启用右键菜单展示；Automator 保存时写的就是这条记录
        let key = format!("\"(null) - {} - runWorkflowAsService\"", s.menu_title);
        let _ = std::process::Command::new("defaults")
            .args([
                "write", "pbs", "NSServicesStatus", "-dict-add", &key,
                r#"{ "enabled_context_menu" = 1; "enabled_services_menu" = 1; "presentation_modes" = { "ContextMenu" = 1; "FinderPreview" = 1; "ServicesMenu" = 1; "TouchBar" = 1; }; }"#,
            ])
            .status();
    }
    refresh_services();
    restart_finder();
    Ok(())
}

pub fn uninstall() -> anyhow::Result<()> {
    for s in SERVICES {
        let wf = workflow_path(s)?;
        if wf.exists() {
            std::fs::remove_dir_all(&wf)?;
        }
    }
    refresh_services();
    Ok(())
}

pub fn installed() -> bool {
    SERVICES
        .iter()
        .all(|s| workflow_path(s).map(|p| p.exists()).unwrap_or(false))
}

fn refresh_services() {
    let _ = std::process::Command::new("/System/Library/CoreServices/pbs")
        .arg("-update")
        .status();
}

fn restart_finder() {
    if std::env::var_os("ORIGAMI_NO_FINDER_RESTART").is_some() {
        return;
    }
    let _ = std::process::Command::new("killall").arg("Finder").status();
}
