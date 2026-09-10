import AppKit

final class BrowserWindowController: NSObject {
    private let client: CPKClient
    private let window: NSWindow
    private let left: PaneViewController
    private let right: PaneViewController
    private var activePane: PaneViewController
    private let status = NSTextField(labelWithString: "Ready")
    private let activity = NSTextField(labelWithString: "No transfer activity yet.")

    init(client: CPKClient) {
        self.client = client
        let home = FileManager.default.homeDirectoryForCurrentUser.path
        let leftPane = PaneViewController(title: "Local A", path: home, client: client)
        let rightPane = PaneViewController(title: "Local B", path: home, client: client)
        left = leftPane
        right = rightPane
        activePane = leftPane
        window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 1320, height: 820),
            styleMask: [.titled, .closable, .miniaturizable, .resizable],
            backing: .buffered,
            defer: false
        )
        super.init()

        left.onBecameActive = { [weak self] in
            guard let self else { return }
            self.activePane = self.left
        }
        right.onBecameActive = { [weak self] in
            guard let self else { return }
            self.activePane = self.right
        }

        configureWindow()
    }

    func show() {
        window.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
    }

    func installMainMenu() {
        let main = NSMenu()
        main.addItem(appMenuItem())
        main.addItem(fileMenuItem())
        main.addItem(viewMenuItem())
        main.addItem(goMenuItem())
        main.addItem(transferMenuItem())
        main.addItem(helpMenuItem())
        NSApp.mainMenu = main
    }

    private func appMenuItem() -> NSMenuItem {
        let item = NSMenuItem()
        let menu = NSMenu()

        menu.addItem(
            withTitle: "About Cyber-Pumpkin",
            action: #selector(showAbout),
            keyEquivalent: ""
        )
        menu.addItem(.separator())
        menu.addItem(
            withTitle: "Quit Cyber-Pumpkin",
            action: #selector(NSApplication.terminate(_:)),
            keyEquivalent: "q"
        )
        item.submenu = menu
        return item
    }

    private func fileMenuItem() -> NSMenuItem {
        let item = NSMenuItem()
        item.title = "File"
        let menu = NSMenu(title: "File")

        addTargetedItem(menu, "New Folder…", #selector(newFolder), "n", [.command, .shift])
        addTargetedItem(menu, "Rename…", #selector(renameSelected), "\r", [])
        addTargetedItem(menu, "Delete…", #selector(deleteSelected), "\u{8}", [])
        menu.addItem(.separator())
        addTargetedItem(menu, "Refresh", #selector(refreshActive), "r", [.command])

        item.submenu = menu
        return item
    }

    private func viewMenuItem() -> NSMenuItem {
        let item = NSMenuItem()
        item.title = "View"
        let menu = NSMenu(title: "View")

        addTargetedItem(
            menu,
            "Show Hidden Files",
            #selector(toggleHiddenMenu(_:)),
            ".",
            [.command]
        )
        addTargetedItem(
            menu,
            "Show Activity",
            #selector(toggleActivityMenu(_:)),
            "",
            []
        )

        item.submenu = menu
        return item
    }

    private func goMenuItem() -> NSMenuItem {
        let item = NSMenuItem()
        item.title = "Go"
        let menu = NSMenu(title: "Go")

        addPlaceItem(menu, "Home", FileManager.default.homeDirectoryForCurrentUser.path)
        addPlaceItem(
            menu,
            "Desktop",
            FileManager.default.homeDirectoryForCurrentUser
                .appendingPathComponent("Desktop")
                .path
        )
        addPlaceItem(
            menu,
            "Documents",
            FileManager.default.homeDirectoryForCurrentUser
                .appendingPathComponent("Documents")
                .path
        )
        addPlaceItem(
            menu,
            "Downloads",
            FileManager.default.homeDirectoryForCurrentUser
                .appendingPathComponent("Downloads")
                .path
        )
        addPlaceItem(menu, "Root", "/")

        item.submenu = menu
        return item
    }

    private func transferMenuItem() -> NSMenuItem {
        let item = NSMenuItem()
        item.title = "Transfer"
        let menu = NSMenu(title: "Transfer")
        addTargetedItem(
            menu,
            "Copy to Other Pane",
            #selector(copyToOtherPane),
            "",
            []
        )
        item.submenu = menu
        return item
    }

    private func helpMenuItem() -> NSMenuItem {
        let item = NSMenuItem()
        item.title = "Help"
        let menu = NSMenu(title: "Help")
        addTargetedItem(
            menu,
            "About Cyber-Pumpkin",
            #selector(showAbout),
            "",
            []
        )
        item.submenu = menu
        return item
    }

    private func addTargetedItem(
        _ menu: NSMenu,
        _ title: String,
        _ action: Selector,
        _ keyEquivalent: String,
        _ modifiers: NSEvent.ModifierFlags
    ) {
        let item = NSMenuItem(
            title: title,
            action: action,
            keyEquivalent: keyEquivalent
        )
        item.target = self
        item.keyEquivalentModifierMask = modifiers
        menu.addItem(item)
    }

    private func addPlaceItem(_ menu: NSMenu, _ title: String, _ path: String) {
        let item = PlaceMenuItem(
            title: title,
            path: path,
            target: self,
            action: #selector(openPlaceMenu(_:))
        )
        menu.addItem(item)
    }

    private func configureWindow() {
        window.title = "Cyber-Pumpkin"
        window.center()

        let sidebar = buildSidebar()
        let browser = buildBrowserContent()

        let split = NSSplitView()
        split.isVertical = true
        split.dividerStyle = .thin
        split.addArrangedSubview(sidebar)
        split.addArrangedSubview(browser)
        sidebar.widthAnchor.constraint(equalToConstant: 190).isActive = true

        window.contentView = split
    }

    private func buildBrowserContent() -> NSView {
        let toolbar = buildToolbar()

        let panes = NSSplitView()
        panes.isVertical = true
        panes.dividerStyle = .thin
        panes.addArrangedSubview(left.view)
        panes.addArrangedSubview(right.view)

        activity.textColor = .secondaryLabelColor
        activity.isHidden = true

        let stack = NSStackView(views: [toolbar, panes, activity])
        stack.orientation = .vertical
        stack.spacing = 6
        stack.edgeInsets = NSEdgeInsets(top: 8, left: 8, bottom: 8, right: 8)
        return stack
    }

    private func buildToolbar() -> NSView {
        let refresh = NSButton(
            title: "Refresh",
            target: self,
            action: #selector(refreshActive)
        )
        let newFolder = NSButton(
            title: "New Folder",
            target: self,
            action: #selector(newFolder)
        )
        let rename = NSButton(
            title: "Rename",
            target: self,
            action: #selector(renameSelected)
        )
        let delete = NSButton(
            title: "Delete",
            target: self,
            action: #selector(deleteSelected)
        )
        let copyLeft = NSButton(
            title: "← Copy",
            target: self,
            action: #selector(copyRightToLeft)
        )
        let copyRight = NSButton(
            title: "Copy →",
            target: self,
            action: #selector(copyLeftToRight)
        )
        let hidden = NSButton(
            checkboxWithTitle: "Hidden",
            target: self,
            action: #selector(toggleHiddenButton(_:))
        )
        let activityButton = NSButton(
            checkboxWithTitle: "Activity",
            target: self,
            action: #selector(toggleActivityButton(_:))
        )

        status.textColor = .secondaryLabelColor
        let stack = NSStackView(views: [
            refresh, newFolder, rename, delete,
            copyLeft, copyRight, hidden, activityButton, status
        ])
        stack.orientation = .horizontal
        stack.spacing = 6
        status.setContentHuggingPriority(.defaultLow, for: .horizontal)
        return stack
    }

    private func buildSidebar() -> NSView {
        let title = NSTextField(labelWithString: "Pumpkin Patch")
        title.font = .boldSystemFont(ofSize: NSFont.systemFontSize)

        let home = FileManager.default.homeDirectoryForCurrentUser
        let places: [(String, URL)] = [
            ("Home", home),
            ("Desktop", home.appendingPathComponent("Desktop")),
            ("Documents", home.appendingPathComponent("Documents")),
            ("Downloads", home.appendingPathComponent("Downloads")),
            ("Root", URL(fileURLWithPath: "/"))
        ]

        let stack = NSStackView()
        stack.orientation = .vertical
        stack.alignment = .leading
        stack.spacing = 4
        stack.edgeInsets = NSEdgeInsets(top: 12, left: 8, bottom: 8, right: 8)
        stack.addArrangedSubview(title)

        for (label, url) in places {
            let button = PlaceButton(
                title: label,
                path: url.path,
                target: self,
                action: #selector(openPlace(_:))
            )
            button.bezelStyle = .inline
            stack.addArrangedSubview(button)
        }
        return stack
    }

    @objc private func openPlace(_ sender: PlaceButton) {
        activePane.navigate(to: sender.path)
    }

    @objc private func openPlaceMenu(_ sender: PlaceMenuItem) {
        activePane.navigate(to: sender.path)
    }

    @objc private func refreshActive() {
        activePane.reloadDirectory()
        status.stringValue = "Refreshed \(activePane.currentPath)"
    }

    @objc private func newFolder() {
        guard let name = prompt(
            title: "New Folder",
            message: "Create a folder in \(activePane.currentPath)",
            initial: ""
        ) else { return }

        guard !name.isEmpty, !name.contains("/") else { return }
        let path = URL(fileURLWithPath: activePane.currentPath)
            .appendingPathComponent(name)
            .path

        do {
            try client.createDirectory(path: path)
            activePane.reloadDirectory()
            status.stringValue = "Created \(name)"
        } catch {
            status.stringValue = "Create failed: \(error.localizedDescription)"
        }
    }

    @objc private func renameSelected() {
        guard let entry = activePane.selectedEntry() else {
            status.stringValue = "Select an item to rename."
            return
        }
        guard let name = prompt(
            title: "Rename",
            message: "Rename \(entry.name)",
            initial: entry.name
        ) else { return }

        guard !name.isEmpty, !name.contains("/") else { return }
        let destination = URL(fileURLWithPath: activePane.currentPath)
            .appendingPathComponent(name)
            .path

        do {
            try client.rename(source: entry.path, destination: destination)
            activePane.reloadDirectory()
            status.stringValue = "Renamed \(entry.name) → \(name)"
        } catch {
            status.stringValue = "Rename failed: \(error.localizedDescription)"
        }
    }

    @objc private func deleteSelected() {
        guard let entry = activePane.selectedEntry() else {
            status.stringValue = "Select an item to delete."
            return
        }

        let alert = NSAlert()
        alert.messageText = "Delete \(entry.name)?"
        alert.informativeText = "Non-empty folders are not removed recursively yet."
        alert.addButton(withTitle: "Delete")
        alert.addButton(withTitle: "Cancel")
        guard alert.runModal() == .alertFirstButtonReturn else { return }

        do {
            try client.remove(path: entry.path)
            activePane.reloadDirectory()
            status.stringValue = "Deleted \(entry.name)"
        } catch {
            status.stringValue = "Delete failed: \(error.localizedDescription)"
        }
    }

    @objc private func copyLeftToRight() {
        copy(source: left, destination: right)
    }

    @objc private func copyRightToLeft() {
        copy(source: right, destination: left)
    }

    @objc private func copyToOtherPane() {
        copy(
            source: activePane,
            destination: activePane === left ? right : left
        )
    }

    private func copy(
        source: PaneViewController,
        destination: PaneViewController
    ) {
        guard let entry = source.selectedEntry(), !entry.isDirectory else {
            status.stringValue = "Select a regular file to copy."
            return
        }

        let destinationPath = URL(fileURLWithPath: destination.currentPath)
            .appendingPathComponent(entry.name)
            .path
        status.stringValue = "Copying \(entry.name)…"

        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            guard let self else { return }
            let result = Result {
                try self.client.copySafe(
                    source: entry.path,
                    destination: destinationPath
                )
            }
            DispatchQueue.main.async {
                switch result {
                case .success:
                    self.status.stringValue = "Copied \(entry.name)"
                    self.activity.stringValue = "Completed • Copy \(entry.name)"
                    destination.reloadDirectory()
                case .failure(let error):
                    self.status.stringValue =
                        "Copy failed: \(error.localizedDescription)"
                    self.activity.stringValue = "Failed • Copy \(entry.name)"
                }
            }
        }
    }

    @objc private func toggleHiddenButton(_ sender: NSButton) {
        setHidden(sender.state == .on)
    }

    @objc private func toggleHiddenMenu(_ sender: NSMenuItem) {
        let next = sender.state != .on
        sender.state = next ? .on : .off
        setHidden(next)
    }

    private func setHidden(_ show: Bool) {
        left.showHidden = show
        right.showHidden = show
    }

    @objc private func toggleActivityButton(_ sender: NSButton) {
        activity.isHidden = sender.state != .on
    }

    @objc private func toggleActivityMenu(_ sender: NSMenuItem) {
        let next = sender.state != .on
        sender.state = next ? .on : .off
        activity.isHidden = !next
    }

    private func prompt(
        title: String,
        message: String,
        initial: String
    ) -> String? {
        let alert = NSAlert()
        alert.messageText = title
        alert.informativeText = message

        let field = NSTextField(string: initial)
        field.frame = NSRect(x: 0, y: 0, width: 280, height: 24)
        field.selectText(nil)
        alert.accessoryView = field
        alert.addButton(withTitle: "OK")
        alert.addButton(withTitle: "Cancel")

        guard alert.runModal() == .alertFirstButtonReturn else { return nil }
        return field.stringValue
    }

    @objc private func showAbout() {
        NSApp.orderFrontStandardAboutPanel(nil)
    }
}

final class PlaceButton: NSButton {
    let path: String

    init(
        title: String,
        path: String,
        target: AnyObject?,
        action: Selector?
    ) {
        self.path = path
        super.init(frame: .zero)
        self.title = title
        self.target = target
        self.action = action
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("init(coder:) is not supported")
    }
}

final class PlaceMenuItem: NSMenuItem {
    let path: String

    init(
        title: String,
        path: String,
        target: AnyObject?,
        action: Selector?
    ) {
        self.path = path
        super.init(
            title: title,
            action: action,
            keyEquivalent: ""
        )
        self.target = target
    }

    @available(*, unavailable)
    required init(coder: NSCoder) {
        fatalError("init(coder:) is not supported")
    }
}
