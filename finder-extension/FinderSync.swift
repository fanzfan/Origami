import Cocoa
import FinderSync

/// Finder Sync 扩展：在 Finder 右键菜单加一个「Origami」入口（带应用图标），
/// 子菜单里是压缩 / 解压动作——与 Windows 右键结构一致（Origami ▸ 四项动词）。
/// 动作把选中文件路径用 URL-safe base64 编码进 origami:// 深链，由主应用接管
/// （与现有 Services / Windows 右键流程共用同一套后端逻辑）。
///
/// 相比 Automator Services：可按代码动态决定项（不受系统 UTI 限制），
/// 且不依赖 pbs 注册那套易碎机制。
@objc(OrigamiFinderSyncExt)
class OrigamiFinderSyncExt: FIFinderSync {
    /// 归档扩展名（含单层压缩与常见复合后缀的末段）。仅用于决定是否显示「解压」；
    /// 主应用会再按魔数精确识别，因此这里宽松即可。
    private static let archiveExts: Set<String> = [
        "zip", "jar", "apk", "7z", "rar", "tar",
        "gz", "tgz", "bz2", "tbz", "tbz2", "xz", "txz", "zst", "tzst",
    ]

    override init() {
        super.init()
        // 监听根目录 => 扩展在所有位置生效（FinderSync 的右键菜单只对被监听目录出现）。
        FIFinderSyncController.default().directoryURLs = [URL(fileURLWithPath: "/")]
    }

    override func menu(for menuKind: FIMenuKind) -> NSMenu {
        let menu = NSMenu(title: "")
        guard menuKind == .contextualMenuForItems else { return menu }
        let urls = selection()
        guard !urls.isEmpty else { return menu }

        // 与 Windows 一致：单个顶层「Origami」入口（带应用图标），四项动词收进其子菜单。
        let root = NSMenuItem(title: "Origami", action: nil, keyEquivalent: "")
        root.image = Self.appIcon
        let sub = NSMenu(title: "")

        // 压缩：对任意选择可用（精简为「ZIP」与「详细设置」两项，其余格式在应用内选）。
        sub.addItem(item("压缩为 ZIP", #selector(zipAction(_:))))
        sub.addItem(item("压缩（详细设置…）", #selector(createAskAction(_:))))

        // 解压：仅当选中项全部是归档时出现（「智能解压」与「解压到…」两项）。
        if urls.allSatisfy(Self.isArchive) {
            sub.addItem(item("智能解压", #selector(extractSmartAction(_:))))
            sub.addItem(item("解压到…", #selector(extractAskAction(_:))))
        }

        root.submenu = sub
        menu.addItem(root)
        return menu
    }

    /// 主应用图标，用于「Origami」入口项。appex 位于 Origami.app/Contents/PlugIns/ 内，
    /// 向上两级到 Contents 再取 Resources/icon.icns（读自身包内文件，沙箱下允许）。
    private static let appIcon: NSImage? = {
        let iconURL = Bundle.main.bundleURL
            .deletingLastPathComponent()      // PlugIns
            .deletingLastPathComponent()      // Contents
            .appendingPathComponent("Resources/icon.icns")
        guard let img = NSImage(contentsOf: iconURL) else { return nil }
        img.size = NSSize(width: 16, height: 16)
        return img
    }()

    // MARK: - 动作

    @objc func zipAction(_ sender: AnyObject?) { open("create", "format", "zip") }
    @objc func createAskAction(_ sender: AnyObject?) { open("create", "format", "ask") }
    @objc func extractSmartAction(_ sender: AnyObject?) { open("extract", "mode", "smart") }
    @objc func extractAskAction(_ sender: AnyObject?) { open("extract", "mode", "ask") }

    // MARK: - 辅助

    private func item(_ title: String, _ sel: Selector) -> NSMenuItem {
        let m = NSMenuItem(title: title, action: sel, keyEquivalent: "")
        m.target = self
        return m
    }

    private func selection() -> [URL] {
        let ctrl = FIFinderSyncController.default()
        var urls = ctrl.selectedItemURLs() ?? []
        if urls.isEmpty, let target = ctrl.targetedURL() { urls = [target] }
        return urls
    }

    private static func isArchive(_ url: URL) -> Bool {
        archiveExts.contains(url.pathExtension.lowercased())
    }

    /// 构造 origami://<base>?<key>=<value>&p=…&p=… 深链并交给主应用。
    private func open(_ base: String, _ key: String, _ value: String) {
        let urls = selection()
        guard !urls.isEmpty else { return }
        var str = "origami://\(base)?\(key)=\(value)"
        for url in urls {
            let encoded = Data(url.path.utf8).base64EncodedString()
                .replacingOccurrences(of: "+", with: "-")
                .replacingOccurrences(of: "/", with: "_")
                .replacingOccurrences(of: "=", with: "")
            str += "&p=\(encoded)"
        }
        if let deepLink = URL(string: str) {
            NSWorkspace.shared.open(deepLink)
        }
    }
}
