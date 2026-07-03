import Cocoa
import FinderSync

/// Finder Sync 扩展：在 Finder 右键的「一级菜单」直接显示 Origami 的压缩 / 解压动作。
/// 菜单动作把选中文件路径用 URL-safe base64 编码进 origami:// 深链，由主应用接管
/// （与现有 Services / Windows 右键流程共用同一套后端逻辑）。
///
/// 相比 Automator Services：菜单出现在一级、可按代码动态决定项（不受系统 UTI 限制），
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

        // 平铺顶层动词（与 Windows 右键逻辑一致：不套「用 Origami 压缩 ▸」级联，少一层）。
        // 压缩：对任意选择可用（精简为「ZIP」与「详细设置」两项，其余格式在应用内选）。
        menu.addItem(item("用 Origami 压缩为 ZIP", #selector(zipAction(_:))))
        menu.addItem(item("用 Origami 压缩（详细设置…）", #selector(createAskAction(_:))))

        // 解压：仅当选中项全部是归档时出现（「当前文件夹」与「解压到…」两项）。
        if urls.allSatisfy(Self.isArchive) {
            menu.addItem(item("用 Origami 解压到当前文件夹", #selector(extractHereAction(_:))))
            menu.addItem(item("用 Origami 解压到…", #selector(extractAskAction(_:))))
        }
        return menu
    }

    // MARK: - 动作

    @objc func zipAction(_ sender: AnyObject?) { open("create", "format", "zip") }
    @objc func createAskAction(_ sender: AnyObject?) { open("create", "format", "ask") }
    @objc func extractHereAction(_ sender: AnyObject?) { open("extract", "mode", "here") }
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
