import AppKit

final class BrowserWindowController: NSObject {
    private let window: NSWindow
    private let left: PaneViewController
    private let right: PaneViewController
    private var activePane: PaneViewController
    private let status = NSTextField(labelWithString: "Ready")
    private let activity = NSTextField(labelWithString: "No transfer activity yet.")

    init(client: CPKClient) {
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

        let appItem = NSMenuItem()
        let appMenu = NSMenu()
        appMenu.addItem(
            withTitle: "About Cyber-Pumpkin",
            action: #selector(showAbout),
            keyEquivalent: ""
        )
        appMenu.addItem(.separator())
        appMenu.addItem(
            withTitle: "Quit Cyber-Pumpkin",
            action: #selector(NSApplication.terminate(_:)),
            keyEquivalent: "q"
        )
        appItem.submenu = appMenu
        main.addItem(appItem)

        let fileItem = NSMenuItem()
        fileItem.title = "File"
        let fileMenu = NSMenu(title: "File")
        let refresh = NSMenuItem(
            title: "Refresh",
            action: #selector(refreshActive),
            keyEquivalent: "r"
        )
        refresh.target = self
        fileMenu.addItem(refresh)
        fileItem.submenu = fileMenu
        main.addItem(fileItem)

        let viewItem = NSMenuItem()
        viewItem.title = "View"
        let viewMenu = NSMenu(title: "View")
        let hidden = NSMenuItem(
            title: "Show Hidden Files",
            action: #selector(toggleHiddenMenu(_:)),
            keyEquivalent: "."
        )
        hidden.target = self
        viewMenu.addItem(hidden)
        let activityItem = NSMenuItem(
            title: "Show Activity",
            action: #selector(toggleActivityMenu(_:)),
            keyEquivalent: ""
        )
        activityItem.target = self
        viewMenu.addItem(activityItem)
        viewItem.submenu = viewMenu
        main.addItem(viewItem)

        NSApp.mainMenu = main
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
        let stack = NSStackView(views: [refresh, hidden, activityButton, status])
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

    @objc private func refreshActive() {
        activePane.reloadDirectory()
        status.stringValue = "Refreshed \(activePane.currentPath)"
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

    @objc private func showAbout() {
        NSApp.orderFrontStandardAboutPanel(nil)
    }
}

final class PlaceButton: NSButton {
    let path: String

    init(title: String, path: String, target: AnyObject?, action: Selector?) {
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
