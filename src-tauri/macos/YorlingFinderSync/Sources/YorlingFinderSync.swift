import AppKit
import Darwin
import FinderSync
import Foundation

private enum YorlingFinderAction: String {
    case newTxt
    case newMd
    case openTerminal
    case copyPath
    case moveTo
    case toggleHiddenFiles
    case snapToGrid
    case openYorling
    case refreshDirectories
}

private struct YorlingFinderContext {
    let menuKind: FIMenuKind
    let targetedURL: URL?
    let selectedItemURLs: [URL]

    init(menuKind: FIMenuKind, targetedURL: URL?, selectedItemURLs: [URL]) {
        self.menuKind = menuKind
        self.targetedURL = Self.standardizedFileURL(targetedURL)
        self.selectedItemURLs = selectedItemURLs.compactMap(Self.standardizedFileURL)
    }

    var effectiveURLs: [URL] {
        if !selectedItemURLs.isEmpty {
            return selectedItemURLs
        }
        if isContainerMenu {
            return targetDirectory.map { [$0] } ?? []
        }
        if let targetedURL {
            return [targetedURL]
        }
        return []
    }

    var movableURLs: [URL] {
        if !selectedItemURLs.isEmpty {
            return selectedItemURLs
        }
        if !isContainerMenu, let targetedURL {
            return [targetedURL]
        }
        return []
    }

    var targetDirectory: URL? {
        if isContainerMenu, let targetedURL, Self.isBrowsableDirectory(targetedURL) {
            return targetedURL
        }
        if let firstSelection = selectedItemURLs.first {
            return Self.directoryURL(for: firstSelection)
        }
        if let targetedURL {
            return Self.directoryURL(for: targetedURL)
        }
        return nil
    }

    var folderArrangementDirectory: URL? {
        if isContainerMenu, let targetedURL, Self.isBrowsableDirectory(targetedURL) {
            return targetedURL
        }
        if let firstSelection = selectedItemURLs.first {
            return firstSelection.deletingLastPathComponent()
        }
        if let targetedURL {
            return targetedURL.deletingLastPathComponent()
        }
        return nil
    }

    var isContainerMenu: Bool {
        menuKind == .contextualMenuForContainer
    }

    var supportsFileActions: Bool {
        targetDirectory != nil || !effectiveURLs.isEmpty
    }

    var supportsNewFile: Bool {
        guard let targetDirectory else {
            return false
        }

        return Self.isWritableDirectory(targetDirectory)
    }

    var supportsOpenTerminal: Bool {
        targetDirectory != nil
    }

    var supportsCopyPath: Bool {
        !effectiveURLs.isEmpty || targetDirectory != nil
    }

    var supportsFolderArrangement: Bool {
        menuKind != .toolbarItemMenu
            && menuKind != .contextualMenuForSidebar
            && folderArrangementDirectory != nil
    }

    var supportsMoveTo: Bool {
        menuKind != .toolbarItemMenu
            && menuKind != .contextualMenuForSidebar
            && !movableURLs.isEmpty
    }

    private static func standardizedFileURL(_ url: URL?) -> URL? {
        guard let url, url.isFileURL else {
            return nil
        }

        return url.standardizedFileURL
    }

    private static func directoryURL(for url: URL) -> URL {
        if isBrowsableDirectory(url) {
            return url
        }
        return url.deletingLastPathComponent()
    }

    private static func isBrowsableDirectory(_ url: URL) -> Bool {
        isDirectory(url) && !isPackage(url)
    }

    private static func isDirectory(_ url: URL) -> Bool {
        var isDirectory = ObjCBool(false)
        FileManager.default.fileExists(atPath: url.path, isDirectory: &isDirectory)
        return isDirectory.boolValue
    }

    private static func isWritableDirectory(_ url: URL) -> Bool {
        isBrowsableDirectory(url) && FileManager.default.isWritableFile(atPath: url.path)
    }

    private static func isPackage(_ url: URL) -> Bool {
        if let values = try? url.resourceValues(forKeys: [.isPackageKey]),
           let isPackage = values.isPackage
        {
            return isPackage
        }

        return ["app", "appex", "bundle", "framework", "kext", "plugin"]
            .contains(url.pathExtension.lowercased())
    }
}

@objc(YorlingFinderSync)
final class YorlingFinderSync: FIFinderSync {
    private static let finderActionRequestNotification = Notification.Name("com.yorling.app.finder-action-request")
    private static let finderActionRequestFileName = "finder-action-request.json"
    private static let runtimeLeaseMaxAgeMillis: UInt64 = 45_000

    private var lastContext: YorlingFinderContext?
    private var titleToAction: [String: YorlingFinderAction] = [:]
    private var tagToAction: [Int: YorlingFinderAction] = [:]
    private var nextMenuTag = 10_000
    private var monitoredDirectoryRefreshTimer: Timer?
    private var monitoredDirectoryObservers: [NSObjectProtocol] = []
    private var lastMonitoredDirectoryPaths = Set<String>()

    override init() {
        super.init()
        configureMonitoredDirectories()
        startMonitoredDirectoryRefresh()
        log("initialized")
    }

    deinit {
        monitoredDirectoryRefreshTimer?.invalidate()
        let notificationCenter = NSWorkspace.shared.notificationCenter
        monitoredDirectoryObservers.forEach(notificationCenter.removeObserver)
    }

    override var toolbarItemName: String {
        "Yorling"
    }

    override var toolbarItemToolTip: String {
        localized("Yorling 快捷操作", "Yorling Quick Actions")
    }

    override var toolbarItemImage: NSImage {
        let image = NSImage(
            systemSymbolName: "bolt.circle.fill",
            accessibilityDescription: "Yorling"
        ) ?? NSImage()
        image.isTemplate = true
        return image
    }

    override func menu(for menuKind: FIMenuKind) -> NSMenu? {
        guard isSuperRightClickEnabled() else {
            log("menu skipped because Super Right Click is disabled")
            return nil
        }

        resetMenuActionLookup()

        let controller = FIFinderSyncController.default()
        let context = YorlingFinderContext(
            menuKind: menuKind,
            targetedURL: controller.targetedURL()?.standardizedFileURL,
            selectedItemURLs: (controller.selectedItemURLs() ?? []).map(\.standardizedFileURL)
        )
        lastContext = context
        log("menu kind=\(menuKind.rawValue) target=\(context.targetedURL?.path ?? "nil") selected=\(context.selectedItemURLs.map(\.path))")

        let menu = NSMenu(title: "Yorling")
        if menuKind == .toolbarItemMenu {
            appendItem(.openYorling, to: menu, title: localized("打开 Yorling", "Open Yorling"), symbolName: "app")
            appendItem(.refreshDirectories, to: menu, title: localized("刷新 Finder 覆盖范围", "Refresh Finder Coverage"), symbolName: "arrow.clockwise")
            return menu
        }

        if context.supportsFileActions {
            if context.supportsNewFile {
                appendNewFileSubmenu(to: menu)
            }
            if context.supportsOpenTerminal {
                appendItem(.openTerminal, to: menu, title: localized("打开终端", "Open Terminal"), symbolName: "terminal")
            }
            if context.supportsCopyPath {
                appendItem(.copyPath, to: menu, title: localized("复制路径", "Copy Path"), symbolName: "doc.on.doc")
            }
            if context.supportsMoveTo {
                appendItem(.moveTo, to: menu, title: localized("移动到...", "Move To..."), symbolName: "folder")
            }
        } else {
            log("menu using limited actions because Finder did not provide a usable file-system target")
        }

        appendItem(.toggleHiddenFiles, to: menu, title: localized("显示/隐藏隐藏文件", "Show/Hide Hidden Files"), symbolName: "eye")

        if context.supportsFolderArrangement {
            appendItem(.snapToGrid, to: menu, title: localized("吸附到网格", "Snap to Grid"), symbolName: "square.grid.3x3")
        }

        appendItem(.openYorling, to: menu, title: localized("打开 Yorling", "Open Yorling"), symbolName: "app")
        if !context.supportsFileActions {
            appendItem(.refreshDirectories, to: menu, title: localized("刷新 Finder 覆盖范围", "Refresh Finder Coverage"), symbolName: "arrow.clockwise")
        }
        return menu
    }

    override func beginObservingDirectory(at url: URL) {
        log("begin observing \(url.path)")
    }

    override func endObservingDirectory(at url: URL) {
        log("end observing \(url.path)")
    }

    @objc private func handleMenuAction(_ sender: NSMenuItem) {
        guard let action = resolveAction(from: sender) else {
            log("action resolution failed title=\(sender.title) tag=\(sender.tag) representedObject=\(String(describing: sender.representedObject)) identifier=\(sender.identifier?.rawValue ?? "nil")")
            return
        }

        let context = currentContext()
        log("performing \(action.rawValue) title=\(sender.title) target=\(context.targetDirectory?.path ?? "nil") selected=\(context.selectedItemURLs.map(\.path))")
        switch action {
        case .newTxt:
            createNewFile(extension: "txt", context: context)
        case .newMd:
            createNewFile(extension: "md", context: context)
        case .openTerminal:
            openTerminal(context: context)
        case .copyPath:
            copyPath(context: context)
        case .moveTo:
            queueMainAppAction(action, context: context)
        case .toggleHiddenFiles:
            queueMainAppAction(action, context: context)
        case .snapToGrid:
            queueMainAppAction(action, context: context)
        case .openYorling:
            openContainingApp()
        case .refreshDirectories:
            configureMonitoredDirectories()
        }
    }

    private func resetMenuActionLookup() {
        titleToAction.removeAll()
        tagToAction.removeAll()
        nextMenuTag = 10_000
    }

    private func resolveAction(from sender: NSMenuItem) -> YorlingFinderAction? {
        if
            let rawAction = sender.representedObject as? String,
            let action = YorlingFinderAction(rawValue: rawAction)
        {
            return action
        }

        if
            let rawIdentifier = sender.identifier?.rawValue,
            let rawAction = rawIdentifier.split(separator: ".").last.map(String.init),
            let action = YorlingFinderAction(rawValue: rawAction)
        {
            return action
        }

        if let action = tagToAction[sender.tag] {
            return action
        }

        return titleToAction[sender.title]
    }

    private func appendNewFileSubmenu(to menu: NSMenu) {
        let submenu = NSMenu(title: localized("新建", "New"))
        appendItem(.newTxt, to: submenu, title: localized("文本文件 (.txt)", "Text File (.txt)"), symbolName: "doc.badge.plus")
        appendItem(.newMd, to: submenu, title: localized("Markdown 文件 (.md)", "Markdown File (.md)"), symbolName: "doc.richtext")

        let item = NSMenuItem(title: localized("新建文件", "New File"), action: nil, keyEquivalent: "")
        item.image = menuImage(symbolName: "doc.badge.plus")
        item.submenu = submenu
        menu.addItem(item)
    }

    private func appendItem(
        _ action: YorlingFinderAction,
        to menu: NSMenu,
        title: String,
        symbolName: String
    ) {
        let item = NSMenuItem(
            title: title,
            action: #selector(handleMenuAction(_:)),
            keyEquivalent: action == .copyPath ? "c" : ""
        )
        let tag = nextMenuTag
        nextMenuTag += 1
        item.target = self
        item.tag = tag
        item.identifier = NSUserInterfaceItemIdentifier("yorling.finder-action.\(action.rawValue)")
        item.representedObject = action.rawValue
        item.image = menuImage(symbolName: symbolName)
        titleToAction[title] = action
        tagToAction[tag] = action
        menu.addItem(item)
    }

    private func currentContext() -> YorlingFinderContext {
        if let lastContext {
            return lastContext
        }

        let controller = FIFinderSyncController.default()
        return YorlingFinderContext(
            menuKind: .contextualMenuForItems,
            targetedURL: controller.targetedURL()?.standardizedFileURL,
            selectedItemURLs: (controller.selectedItemURLs() ?? []).map(\.standardizedFileURL)
        )
    }

    private func configureMonitoredDirectories() {
        let directories = monitoredDirectoryURLs()
        FIFinderSyncController.default().directoryURLs = Set(directories)

        let directoryPaths = Set(directories.map(\.path))
        if directoryPaths != lastMonitoredDirectoryPaths {
            lastMonitoredDirectoryPaths = directoryPaths
            log("monitoring \(directories.map(\.path).joined(separator: ", "))")
        }
    }

    private func startMonitoredDirectoryRefresh() {
        let notificationCenter = NSWorkspace.shared.notificationCenter
        let notificationNames: [Notification.Name] = [
            NSWorkspace.didMountNotification,
            NSWorkspace.didUnmountNotification,
            NSWorkspace.didRenameVolumeNotification,
            NSWorkspace.didWakeNotification,
        ]

        monitoredDirectoryObservers = notificationNames.map { name in
            notificationCenter.addObserver(
                forName: name,
                object: nil,
                queue: .main
            ) { [weak self] _ in
                self?.configureMonitoredDirectories()
            }
        }

        let timer = Timer(timeInterval: 60, repeats: true) { [weak self] _ in
            self?.configureMonitoredDirectories()
        }
        timer.tolerance = 10
        RunLoop.main.add(timer, forMode: .common)
        monitoredDirectoryRefreshTimer = timer
    }

    private func monitoredDirectoryURLs() -> [URL] {
        var candidates = [
            realHomeDirectory(),
            URL(fileURLWithPath: "/Users", isDirectory: true),
            URL(fileURLWithPath: "/Users/Shared", isDirectory: true),
            URL(fileURLWithPath: "/", isDirectory: true),
            URL(fileURLWithPath: "/Applications", isDirectory: true),
            URL(fileURLWithPath: "/System/Applications", isDirectory: true),
            URL(fileURLWithPath: "/System/Applications/Utilities", isDirectory: true),
            URL(fileURLWithPath: "/System/Library/CoreServices/Applications", isDirectory: true),
            URL(fileURLWithPath: "/Volumes", isDirectory: true),
        ]
        candidates.append(contentsOf: userDirectoryURLs())
        candidates.append(contentsOf: mountedVolumeRootURLs())

        var seen = Set<String>()
        return candidates.compactMap { candidate in
            let standardized = candidate.standardizedFileURL
            var isDirectory = ObjCBool(false)
            guard FileManager.default.fileExists(atPath: standardized.path, isDirectory: &isDirectory),
                  isDirectory.boolValue
            else {
                return nil
            }

            let key = standardized.path
            guard !seen.contains(key) else {
                return nil
            }

            seen.insert(key)
            return standardized
        }
    }

    private func userDirectoryURLs() -> [URL] {
        let searchDirectories: [FileManager.SearchPathDirectory] = [
            .desktopDirectory,
            .documentDirectory,
            .downloadsDirectory,
            .moviesDirectory,
            .musicDirectory,
            .picturesDirectory,
            .applicationDirectory,
        ]

        return searchDirectories.compactMap {
            FileManager.default.urls(for: $0, in: .userDomainMask).first
        }
    }

    private func mountedVolumeRootURLs() -> [URL] {
        let resourceKeys: [URLResourceKey] = [.volumeIsBrowsableKey]
        guard let volumeURLs = FileManager.default.mountedVolumeURLs(
            includingResourceValuesForKeys: resourceKeys,
            options: []
        ) else {
            return []
        }

        return volumeURLs.filter { volumeURL in
            let values = try? volumeURL.resourceValues(forKeys: Set(resourceKeys))
            return values?.volumeIsBrowsable != false
        }
    }

    private func createNewFile(extension fileExtension: String, context: YorlingFinderContext) {
        guard let directory = context.targetDirectory else {
            return
        }
        guard FileManager.default.isWritableFile(atPath: directory.path) else {
            log("new file skipped because target is not writable: \(directory.path)")
            return
        }

        let fileURL = uniqueFileURL(in: directory, extension: fileExtension)
        let created = FileManager.default.createFile(atPath: fileURL.path, contents: Data(), attributes: nil)
        if created {
            NSWorkspace.shared.activateFileViewerSelecting([fileURL])
            log("created \(fileURL.path)")
        } else {
            log("failed to create \(fileURL.path)")
        }
    }

    private func uniqueFileURL(in directory: URL, extension fileExtension: String) -> URL {
        let base = directory.appendingPathComponent("Untitled").appendingPathExtension(fileExtension)
        if !FileManager.default.fileExists(atPath: base.path) {
            return base
        }

        for index in 1..<1000 {
            let candidate = directory
                .appendingPathComponent("Untitled \(index)")
                .appendingPathExtension(fileExtension)
            if !FileManager.default.fileExists(atPath: candidate.path) {
                return candidate
            }
        }

        return directory
            .appendingPathComponent("Untitled-\(Int(Date().timeIntervalSince1970))")
            .appendingPathExtension(fileExtension)
    }

    private func openTerminal(context: YorlingFinderContext) {
        guard let directory = context.targetDirectory else {
            return
        }

        switch selectedTerminalID() {
        case "iterm2":
            if !openApplication(bundleIdentifier: "com.googlecode.iterm2", urls: [directory]) {
                if runProcess("/usr/bin/open", arguments: ["-a", "iTerm", directory.path]) != 0 {
                    openTerminalApp(at: directory)
                }
            }
        case "ghostty":
            if !openApplicationWithArguments(
                bundleIdentifier: "com.mitchellh.ghostty",
                appName: "Ghostty",
                arguments: ["--working-directory=\(directory.path)"]
            ) {
                openTerminalApp(at: directory)
            }
        case "wezterm":
            if !openApplicationWithArguments(
                bundleIdentifier: "com.github.wez.wezterm",
                appName: "WezTerm",
                arguments: ["start", "--cwd", directory.path]
            ) {
                openTerminalApp(at: directory)
            }
        case "alacritty":
            if !openApplicationWithArguments(
                bundleIdentifier: "org.alacritty",
                appName: "Alacritty",
                arguments: ["--working-directory", directory.path]
            ) {
                openTerminalApp(at: directory)
            }
        case "kitty":
            if !openApplicationWithArguments(
                bundleIdentifier: "net.kovidgoyal.kitty",
                appName: "kitty",
                arguments: ["--directory", directory.path]
            ) {
                openTerminalApp(at: directory)
            }
        case "warp":
            if runProcess("/usr/bin/open", arguments: ["-a", "Warp", directory.path]) != 0 {
                openTerminalApp(at: directory)
            }
        default:
            openTerminalApp(at: directory)
        }
    }

    private func copyPath(context: YorlingFinderContext) {
        let urls = context.effectiveURLs.isEmpty
            ? context.targetDirectory.map { [$0] } ?? []
            : context.effectiveURLs
        let paths = urls.map(\.path).joined(separator: "\n")
        guard !paths.isEmpty else {
            return
        }

        NSPasteboard.general.clearContents()
        let copied = NSPasteboard.general.setString(paths, forType: .string)
        log("copyPath copied=\(copied) paths=\(paths)")
    }

    private func queueMainAppAction(_ action: YorlingFinderAction, context: YorlingFinderContext) {
        let selectedPaths: [String]
        if action == .snapToGrid {
            selectedPaths = context.selectedItemURLs.map(\.path)
        } else if action == .moveTo {
            selectedPaths = context.movableURLs.map(\.path)
        } else {
            selectedPaths = context.effectiveURLs.map(\.path)
        }
        var payload: [String: Any] = [
            "id": UUID().uuidString,
            "action": action.rawValue,
            "created_at": Int(Date().timeIntervalSince1970 * 1000),
            "selected_paths": selectedPaths,
        ]
        if action == .snapToGrid, let folderArrangementDirectory = context.folderArrangementDirectory {
            payload["directory_path"] = folderArrangementDirectory.path
        } else if let targetDirectory = context.targetDirectory {
            payload["directory_path"] = targetDirectory.path
        }
        if let targetedURL = context.targetedURL {
            payload["targeted_path"] = targetedURL.path
        }

        do {
            let requestURL = try finderActionRequestURL()
            let data = try JSONSerialization.data(withJSONObject: payload, options: [.prettyPrinted])
            try data.write(to: requestURL, options: .atomic)
            DistributedNotificationCenter.default().post(name: Self.finderActionRequestNotification, object: nil)
            let appRunning = containingAppIsRunning()
            if !appRunning {
                openContainingAppForFinderAction()
            }
            log("queued main app action=\(action.rawValue) request=\(requestURL.path) directory=\(context.targetDirectory?.path ?? "nil") appRunning=\(appRunning)")
        } catch {
            log("failed to queue main app action=\(action.rawValue): \(error.localizedDescription)")
        }
    }

    private func finderActionRequestURL() throws -> URL {
        let directory = URL(fileURLWithPath: "/Users/Shared", isDirectory: true)
            .appendingPathComponent("Yorling", isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        return directory.appendingPathComponent(Self.finderActionRequestFileName, isDirectory: false)
    }

    private func openContainingApp(activates: Bool = true) {
        let appURL = containingAppBundleURL()
        let configuration = NSWorkspace.OpenConfiguration()
        configuration.activates = activates
        NSWorkspace.shared.openApplication(at: appURL, configuration: configuration) { _, error in
            if let error {
                self.log("failed to open containing app: \(error.localizedDescription)")
            }
        }
    }

    private func openContainingAppForFinderAction() {
        let appURL = containingAppBundleURL()
        if runProcess("/usr/bin/open", arguments: ["-g", "-j", appURL.path]) == 0 {
            return
        }

        openContainingApp(activates: false)
    }

    private func containingAppIsRunning() -> Bool {
        guard let bundleIdentifier = Bundle(url: containingAppBundleURL())?.bundleIdentifier else {
            return false
        }

        return !NSRunningApplication.runningApplications(withBundleIdentifier: bundleIdentifier).isEmpty
    }

    private func containingAppBundleURL() -> URL {
        Bundle.main.bundleURL
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
    }

    private func realHomeDirectory() -> URL {
        if let entry = getpwuid(getuid()) {
            return URL(fileURLWithPath: String(cString: entry.pointee.pw_dir), isDirectory: true)
                .standardizedFileURL
        }
        return FileManager.default.homeDirectoryForCurrentUser.standardizedFileURL
    }

    private func isSuperRightClickEnabled() -> Bool {
        let state = sharedState()
        return (state["enabled"] as? Bool) ?? false
    }

    private func runtimeLeaseIsCurrent(in state: [String: Any]) -> Bool {
        guard (state["runtime_active"] as? Bool) ?? false else {
            return false
        }

        guard let updatedAt = (state["runtime_updated_at"] as? NSNumber)?.uint64Value,
              updatedAt > 0
        else {
            return false
        }

        let now = UInt64(Date().timeIntervalSince1970 * 1000)
        if updatedAt > now + 5_000 {
            return false
        }

        return now - updatedAt <= Self.runtimeLeaseMaxAgeMillis
    }

    private func selectedTerminalID() -> String {
        (sharedState()["terminal_id"] as? String) ?? "terminal"
    }

    private func sharedState() -> [String: Any] {
        let stateURL = realHomeDirectory()
            .appendingPathComponent("Library", isDirectory: true)
            .appendingPathComponent("Application Support", isDirectory: true)
            .appendingPathComponent("Yorling", isDirectory: true)
            .appendingPathComponent("super-right-click.json", isDirectory: false)

        guard
            let data = try? Data(contentsOf: stateURL),
            let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
        else {
            return [:]
        }

        return json
    }

    private func openTerminalApp(at directory: URL) {
        if openApplication(bundleIdentifier: "com.apple.Terminal", urls: [directory]) {
            return
        }

        if runProcess("/usr/bin/open", arguments: ["-a", "Terminal", directory.path]) == 0 {
            return
        }

        let command = "cd \(shellQuoted(directory.path))"
        let script = """
        tell application "Terminal"
          activate
          do script "\(appleScriptEscaped(command))"
        end tell
        """
        _ = runProcess("/usr/bin/osascript", arguments: ["-e", script])
    }

    private func openApplication(bundleIdentifier: String, urls: [URL]) -> Bool {
        guard let applicationURL = NSWorkspace.shared.urlForApplication(withBundleIdentifier: bundleIdentifier) else {
            log("application not found bundleIdentifier=\(bundleIdentifier)")
            return false
        }

        let configuration = NSWorkspace.OpenConfiguration()
        configuration.activates = true
        NSWorkspace.shared.open(urls, withApplicationAt: applicationURL, configuration: configuration) { _, error in
            if let error {
                self.log("failed to open \(bundleIdentifier): \(error.localizedDescription)")
            }
        }
        return true
    }

    private func openApplicationWithArguments(
        bundleIdentifier: String,
        appName: String,
        arguments: [String]
    ) -> Bool {
        var bundleArguments = ["-n", "-b", bundleIdentifier, "--args"]
        bundleArguments.append(contentsOf: arguments)
        if runProcess("/usr/bin/open", arguments: bundleArguments) == 0 {
            return true
        }

        var appArguments = ["-n", "-a", appName, "--args"]
        appArguments.append(contentsOf: arguments)
        return runProcess("/usr/bin/open", arguments: appArguments) == 0
    }

    private func runProcess(_ launchPath: String, arguments: [String]) -> Int32 {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: launchPath)
        process.arguments = arguments

        do {
            try process.run()
            process.waitUntilExit()
            let status = process.terminationStatus
            if status != 0 {
                log("process exited status=\(status) launchPath=\(launchPath) arguments=\(arguments)")
            }
            return status
        } catch {
            log("process failed \(launchPath): \(error.localizedDescription)")
            return -1
        }
    }

    private func menuImage(symbolName: String) -> NSImage? {
        let image = NSImage(systemSymbolName: symbolName, accessibilityDescription: nil)
        image?.isTemplate = true
        return image
    }

    private func localized(_ zh: String, _ en: String) -> String {
        let language = Locale.preferredLanguages.first?.lowercased() ?? ""
        return language.hasPrefix("zh") ? zh : en
    }

    private func appleScriptEscaped(_ value: String) -> String {
        value
            .replacingOccurrences(of: "\\", with: "\\\\")
            .replacingOccurrences(of: "\"", with: "\\\"")
    }

    private func shellQuoted(_ value: String) -> String {
        let allowed = CharacterSet(charactersIn: "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789/-_.:")
        if value.unicodeScalars.allSatisfy({ allowed.contains($0) }) {
            return value
        }
        return "'\(value.replacingOccurrences(of: "'", with: "'\"'\"'"))'"
    }

    private func log(_ message: String) {
        NSLog("[YorlingFinderSync] %@", message)
        appendDebugLog(message)
    }

    private func appendDebugLog(_ message: String) {
        let logDirectory = URL(fileURLWithPath: "/Users/Shared", isDirectory: true)
            .appendingPathComponent("Yorling", isDirectory: true)
        let logURL = logDirectory.appendingPathComponent("finder-sync-debug.log", isDirectory: false)
        let line = "[\(Date())] \(message)\n"

        do {
            try FileManager.default.createDirectory(at: logDirectory, withIntermediateDirectories: true)
            if !FileManager.default.fileExists(atPath: logURL.path) {
                try line.write(to: logURL, atomically: true, encoding: .utf8)
                return
            }

            let handle = try FileHandle(forWritingTo: logURL)
            defer { try? handle.close() }
            try handle.seekToEnd()
            if let data = line.data(using: .utf8) {
                try handle.write(contentsOf: data)
            }
        } catch {
            NSLog("[YorlingFinderSync] failed to append debug log: %@", error.localizedDescription)
        }
    }
}
