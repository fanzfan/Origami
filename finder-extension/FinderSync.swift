import Cocoa
import FinderSync

/// Finder Sync 扩展：在 Finder 右键的「一级菜单」直接显示「用 Origami 压缩 ▸」。
/// 菜单动作把选中文件路径用 URL-safe base64 编码进 origami://create 深链，
/// 由主应用接管（与现有 Services 流程同一套后端逻辑）。
@objc(OrigamiFinderSyncExt)
class OrigamiFinderSyncExt: FIFinderSync {
    override init() {
        super.init()
        // 监听根目录 -> 扩展在所有位置生效。
        FIFinderSyncController.default().directoryURLs = [URL(fileURLWithPath: "/")]
    }

    override func menu(for menuKind: FIMenuKind) -> NSMenu {
        let menu = NSMenu(title: "")
        guard menuKind == .contextualMenuForItems || menuKind == .contextualMenuForContainer
        else { return menu }

        let parent = NSMenuItem(title: "用 Origami 压缩", action: nil, keyEquivalent: "")
        let sub = NSMenu(title: "")
        let zip = NSMenuItem(title: "压缩为 ZIP", action: #selector(zipAction(_:)), keyEquivalent: "")
        zip.target = self
        sub.addItem(zip)
        let sevenz = NSMenuItem(title: "压缩为 7Z", action: #selector(sevenZAction(_:)), keyEquivalent: "")
        sevenz.target = self
        sub.addItem(sevenz)
        let ask = NSMenuItem(title: "压缩（详细设置…）", action: #selector(askAction(_:)), keyEquivalent: "")
        ask.target = self
        sub.addItem(ask)
        parent.submenu = sub
        menu.addItem(parent)
        return menu
    }

    @objc func zipAction(_ sender: AnyObject?) { launch("zip") }
    @objc func sevenZAction(_ sender: AnyObject?) { launch("7z") }
    @objc func askAction(_ sender: AnyObject?) { launch("ask") }

    private func launch(_ format: String) {
        let ctrl = FIFinderSyncController.default()
        var urls = ctrl.selectedItemURLs() ?? []
        if urls.isEmpty, let target = ctrl.targetedURL() {
            urls = [target]
        }
        guard !urls.isEmpty else { return }

        var str = "origami://create?format=\(format)"
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
