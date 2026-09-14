import AppKit

final class BrowserWindowController: NSObject {
    private let client: CPKClient
    private let window: NSWindow
    private let left: PaneViewController
    private let right: PaneViewController
    private var activePane: PaneViewController
    private let status = NSTextField(labelWithString: "Ready")
    private let activity = NSTextField(labelWithString: "No transfer activity yet.")
    private let savedConnections = NSStackView()
    private let inspectorName = NSTextField(labelWithString: "No Selection")
    private let inspectorKind = NSTextField(labelWithString: "—")
    private let inspectorSize = NSTextField(labelWithString: "—")
    private let inspectorModified = NSTextField(labelWithString: "—")
    private let inspectorBackend = NSTextField(labelWithString: "—")
    private let inspectorPath = NSTextField(wrappingLabelWithString: "—")
    private var preferencesController: PreferencesWindowController?
    private var reverseSort = false

    init(client: CPKClient) {
        self.client = client
        let home = FileManager.default.homeDirectoryForCurrentUser.path
        let leftPane = PaneViewController(title: "Local A", path: home, client: client)
        let rightPane = PaneViewController(title: "Local B", path: home, client: client)
        left = leftPane
        right = rightPane
        activePane = leftPane
        window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 1380, height: 860),
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

        left.onSelectionChanged = { [weak self] entry, connection in
            guard let self, self.activePane === self.left else { return }
            self.updateInspector(entry: entry, connection: connection)
        }
        right.onSelectionChanged = { [weak self] entry, connection in
            guard let self, self.activePane === self.right else { return }
            self.updateInspector(entry: entry, connection: connection)
        }

        configureWindow()
        installContextMenus()
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
        item.title = "Cyber-Pumpkin"
        let menu = NSMenu()

        addTargetedItem(
            menu,
            "About Cyber-Pumpkin",
            #selector(showAbout),
            "",
            []
        )
        addTargetedItem(
            menu,
            "Preferences…",
            #selector(showPreferences),
            ",",
            [.command]
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

        addTargetedItem(
            menu,
            "Connect SFTP…",
            #selector(connectSFTP),
            "k",
            [.command]
        )
        addTargetedItem(
            menu,
            "Disconnect to Local",
            #selector(disconnectToLocal),
            "",
            []
        )
        menu.addItem(.separator())
        addTargetedItem(
            menu,
            "New Folder…",
            #selector(newFolder),
            "n",
            [.command, .shift]
        )
        addTargetedItem(
            menu,
            "Get Info",
            #selector(showInfo),
            "i",
            [.command]
        )
        addTargetedItem(
            menu,
            "Rename…",
            #selector(renameSelected),
            "\r",
            []
        )
        addTargetedItem(
            menu,
            "Delete…",
            #selector(deleteSelected),
            "\u{8}",
            []
        )
        menu.addItem(.separator())
        addTargetedItem(
            menu,
            "Refresh",
            #selector(refreshActive),
            "r",
            [.command]
        )

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
        menu.addItem(.separator())

        let sortItem = NSMenuItem()
        sortItem.title = "Sort By"
        sortItem.submenu = sortMenu()
        menu.addItem(sortItem)

        item.submenu = menu
        return item
    }

    private func sortMenu() -> NSMenu {
        let menu = NSMenu(title: "Sort By")
        addTargetedItem(menu, "Name", #selector(sortByName), "", [])
        addTargetedItem(menu, "Type", #selector(sortByType), "", [])
        addTargetedItem(menu, "Size", #selector(sortBySize), "", [])
        menu.addItem(.separator())
        addTargetedItem(
            menu,
            "Reverse Order",
            #selector(toggleSortDirection),
            "",
            []
        )
        return menu
    }

    private func goMenuItem() -> NSMenuItem {
        let item = NSMenuItem()
        item.title = "Go"
        let menu = NSMenu(title: "Go")

        let home = FileManager.default.homeDirectoryForCurrentUser
        addPlaceItem(menu, "Home", home.path)
        addPlaceItem(menu, "Desktop", home.appendingPathComponent("Desktop").path)
        addPlaceItem(menu, "Documents", home.appendingPathComponent("Documents").path)
        addPlaceItem(menu, "Downloads", home.appendingPathComponent("Downloads").path)
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
            "c",
            [.command, .shift]
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

    private func installContextMenus() {
        left.installContextMenu(contextMenu())
        right.installContextMenu(contextMenu())
    }

    private func contextMenu() -> NSMenu {
        let menu = NSMenu()
        addTargetedItem(
            menu,
            "Copy to Other Pane",
            #selector(copyToOtherPane),
            "",
            []
        )
        menu.addItem(.separator())
        addTargetedItem(menu, "Get Info", #selector(showInfo), "", [])
        addTargetedItem(menu, "Rename…", #selector(renameSelected), "", [])
        addTargetedItem(menu, "Delete…", #selector(deleteSelected), "", [])
        return menu
    }

    private func separator() -> NSBox {
        let box = NSBox()
        box.boxType = .separator
        return box
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
        let inspector = buildInspector()

        let workspace = NSSplitView()
        workspace.isVertical = true
        workspace.dividerStyle = .thin
        workspace.addArrangedSubview(browser)
        workspace.addArrangedSubview(inspector)
        inspector.widthAnchor.constraint(equalToConstant: 270).isActive = true

        let split = NSSplitView()
        split.isVertical = true
        split.dividerStyle = .thin
        split.addArrangedSubview(sidebar)
        split.addArrangedSubview(workspace)
        sidebar.widthAnchor.constraint(equalToConstant: 210).isActive = true

        window.contentView = split
        reloadSavedConnections()
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

    private func buildInspector() -> NSView {
        let title = NSTextField(labelWithString: "Inspector")
        title.font = .boldSystemFont(ofSize: NSFont.systemFontSize)

        inspectorName.font = .boldSystemFont(ofSize: NSFont.systemFontSize)
        inspectorPath.textColor = .secondaryLabelColor

        let stack = NSStackView(views: [
            title,
            separator(),
            inspectorName,
            inspectorRow(label: "Type", value: inspectorKind),
            inspectorRow(label: "Size", value: inspectorSize),
            inspectorRow(label: "Modified", value: inspectorModified),
            inspectorRow(label: "Location", value: inspectorBackend),
            separator(),
            NSTextField(labelWithString: "Path"),
            inspectorPath
        ])
        stack.orientation = .vertical
        stack.alignment = .leading
        stack.spacing = 8
        stack.edgeInsets = NSEdgeInsets(top: 12, left: 12, bottom: 12, right: 12)
        return stack
    }

    private func inspectorRow(label: String, value: NSTextField) -> NSView {
        let key = NSTextField(labelWithString: label)
        key.textColor = .secondaryLabelColor
        key.setContentHuggingPriority(.required, for: .horizontal)
        let row = NSStackView(views: [key, value])
        row.orientation = .horizontal
        row.spacing = 8
        return row
    }

    private func updateInspector(
        entry: CPKEntry?,
        connection: BrowserConnection
    ) {
        guard let entry else {
            inspectorName.stringValue = "No Selection"
            inspectorKind.stringValue = "—"
            inspectorSize.stringValue = "—"
            inspectorModified.stringValue = "—"
            inspectorBackend.stringValue = connection.displayName
            inspectorPath.stringValue = "—"
            return
        }

        inspectorName.stringValue = entry.name
        inspectorKind.stringValue = entry.kind
        inspectorSize.stringValue = activePane.formatSize(entry.size)
        inspectorModified.stringValue = activePane.formatDate(entry.modified)
        inspectorBackend.stringValue = connection.displayName
        inspectorPath.stringValue = entry.path
    }

    private func buildToolbar() -> NSView {
        let refresh = NSButton(
            title: "Refresh",
            target: self,
            action: #selector(refreshActive)
        )
        let connect = NSButton(
            title: "Connect",
            target: self,
            action: #selector(connectSFTP)
        )
        let local = NSButton(
            title: "Local",
            target: self,
            action: #selector(disconnectToLocal)
        )
        let newFolder = NSButton(
            title: "New Folder",
            target: self,
            action: #selector(newFolder)
        )
        let info = NSButton(
            title: "Info",
            target: self,
            action: #selector(showInfo)
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
        let sort = NSPopUpButton()
        sort.addItems(withTitles: ["Name", "Type", "Size"])
        sort.target = self
        sort.action = #selector(sortChanged(_:))

        let reverse = NSButton(
            checkboxWithTitle: "Reverse",
            target: self,
            action: #selector(reverseChanged(_:))
        )

        status.textColor = .secondaryLabelColor
        let stack = NSStackView(views: [
            refresh, connect, local, newFolder, info, rename, delete,
            copyLeft, copyRight, hidden, activityButton,
            sort, reverse, status
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

        stack.addArrangedSubview(separator())

        let connectionsTitle = NSTextField(labelWithString: "Connections")
        connectionsTitle.font = .boldSystemFont(ofSize: NSFont.systemFontSize)
        stack.addArrangedSubview(connectionsTitle)

        savedConnections.orientation = .vertical
        savedConnections.alignment = .leading
        savedConnections.spacing = 3
        stack.addArrangedSubview(savedConnections)

        let addConnection = NSButton(
            title: "+ Add Connection",
            target: self,
            action: #selector(connectSFTP)
        )
        addConnection.bezelStyle = .inline
        stack.addArrangedSubview(addConnection)

        let historyTitle = NSTextField(labelWithString: "History")
        historyTitle.font = .boldSystemFont(ofSize: NSFont.systemFontSize)
        stack.addArrangedSubview(historyTitle)
        let history = NSTextField(
            wrappingLabelWithString: "Recent connections will appear here."
        )
        history.textColor = .secondaryLabelColor
        stack.addArrangedSubview(history)

        return stack
    }

    @objc private func openPlace(_ sender: PlaceButton) {
        activePane.navigate(to: sender.path)
    }

    @objc private func openPlaceMenu(_ sender: PlaceMenuItem) {
        activePane.navigate(to: sender.path)
    }

    private func reloadSavedConnections() {
        for view in savedConnections.arrangedSubviews {
            savedConnections.removeArrangedSubview(view)
            view.removeFromSuperview()
        }

        let profiles = (try? client.profiles()) ?? []
        if profiles.isEmpty {
            let empty = NSTextField(labelWithString: "No saved connections yet.")
            empty.textColor = .secondaryLabelColor
            savedConnections.addArrangedSubview(empty)
            return
        }

        for profile in profiles {
            let button = SavedConnectionButton(
                title: profile.name,
                profile: profile,
                target: self,
                action: #selector(openSavedConnection(_:))
            )
            button.bezelStyle = .inline
            button.toolTip =
                "\(profile.username)@\(profile.host):\(profile.port)"
            savedConnections.addArrangedSubview(button)
        }
    }

    @objc private func openSavedConnection(_ sender: SavedConnectionButton) {
        do {
            try activePane.connectSFTP(
                sender.profile.remote,
                path: sender.profile.path
            )
            status.stringValue = "Connected \(sender.profile.name)"
        } catch {
            status.stringValue =
                "Saved connection failed: \(error.localizedDescription)"
        }
    }

    private func profileID(
        name: String,
        host: String,
        username: String
    ) -> String {
        "\(name)-\(username)-\(host)"
            .lowercased()
            .map { character in
                character.isLetter || character.isNumber || character == "-"
                    ? character
                    : "-"
            }
            .reduce(into: "") { $0.append($1) }
    }

    @objc private func refreshActive() {
        activePane.reloadDirectory()
        status.stringValue = "Refreshed \(activePane.currentPath)"
    }

    @objc private func connectSFTP() {
        let alert = NSAlert()
        alert.messageText = "Connect SFTP"
        alert.informativeText =
            "Uses your SSH agent and requires a matching host key in ~/.ssh/known_hosts."

        let displayName = NSTextField(string: "")
        displayName.placeholderString = "Display name (optional)"
        let host = NSTextField(string: "")
        host.placeholderString = "Host"
        let username = NSTextField(string: NSUserName())
        username.placeholderString = "Username"
        let port = NSTextField(string: "22")
        port.placeholderString = "Port"
        let path = NSTextField(string: "/")
        path.placeholderString = "Remote path"

        let saveProfile = NSButton(
            checkboxWithTitle: "Save in Pumpkin Patch",
            target: nil,
            action: nil
        )
        let fields = NSStackView(
            views: [displayName, host, username, port, path, saveProfile]
        )
        fields.orientation = .vertical
        fields.spacing = 6
        fields.frame = NSRect(x: 0, y: 0, width: 320, height: 154)

        alert.accessoryView = fields
        alert.addButton(withTitle: "Connect")
        alert.addButton(withTitle: "Cancel")

        guard alert.runModal() == .alertFirstButtonReturn else {
            return
        }

        guard let portNumber = UInt16(port.stringValue),
              portNumber > 0,
              !host.stringValue.isEmpty,
              !username.stringValue.isEmpty else {
            status.stringValue = "Invalid SFTP connection settings."
            return
        }

        let remote = SFTPConnection(
            host: host.stringValue,
            username: username.stringValue,
            port: portNumber
        )

        do {
            try activePane.connectSFTP(remote, path: path.stringValue)

            if saveProfile.state == .on {
                let name = displayName.stringValue.isEmpty
                    ? host.stringValue
                    : displayName.stringValue
                let profile = CPKSavedProfile(
                    id: profileID(
                        name: name,
                        host: host.stringValue,
                        username: username.stringValue
                    ),
                    name: name,
                    host: host.stringValue,
                    username: username.stringValue,
                    port: portNumber,
                    path: path.stringValue
                )
                try client.saveProfile(profile)
                reloadSavedConnections()
            }

            status.stringValue = "Connected \(remote.displayName)"
        } catch {
            status.stringValue =
                "SFTP connection failed: \(error.localizedDescription)"
        }
    }

    @objc private func disconnectToLocal() {
        let home = FileManager.default.homeDirectoryForCurrentUser.path
        activePane.disconnectToLocal(path: home)
        status.stringValue = "Returned active pane to local filesystem"
    }

    @objc private func newFolder() {
        guard let name = prompt(
            title: "New Folder",
            message: "Create a folder in \(activePane.currentPath)",
            initial: ""
        ) else {
            return
        }

        guard validName(name) else {
            return
        }

        let path = URL(fileURLWithPath: activePane.currentPath)
            .appendingPathComponent(name)
            .path

        do {
            try client.createDirectory(
                connection: activePane.connection,
                path: path
            )
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
        ) else {
            return
        }

        guard validName(name) else {
            return
        }

        let destination = URL(fileURLWithPath: activePane.currentPath)
            .appendingPathComponent(name)
            .path

        do {
            try client.rename(
                connection: activePane.connection,
                source: entry.path,
                destination: destination
            )
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

        guard alert.runModal() == .alertFirstButtonReturn else {
            return
        }

        do {
            try client.remove(
                connection: activePane.connection,
                path: entry.path
            )
            activePane.reloadDirectory()
            status.stringValue = "Deleted \(entry.name)"
        } catch {
            status.stringValue = "Delete failed: \(error.localizedDescription)"
        }
    }

    @objc private func showInfo() {
        guard let entry = activePane.selectedEntry() else {
            status.stringValue = "Select an item to inspect."
            return
        }

        let alert = NSAlert()
        alert.messageText = entry.name
        alert.informativeText = [
            "Type: \(entry.kind)",
            "Size: \(activePane.formatSize(entry.size))",
            "Path: \(entry.path)"
        ].joined(separator: "\n")
        alert.addButton(withTitle: "Close")
        alert.runModal()
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
        guard let entry = source.selectedEntry() else {
            status.stringValue = "Select a file or folder to copy."
            return
        }

        let destinationPath = URL(fileURLWithPath: destination.currentPath)
            .appendingPathComponent(entry.name)
            .path
        status.stringValue = "Copying \(entry.name)…"

        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            guard let self else {
                return
            }

            let result = Result {
                try self.client.copyTree(
                    sourceConnection: source.connection,
                    source: entry.path,
                    destinationConnection: destination.connection,
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

    @objc private func sortChanged(_ sender: NSPopUpButton) {
        let mode: PaneSortMode
        switch sender.indexOfSelectedItem {
        case 1:
            mode = .type
        case 2:
            mode = .size
        default:
            mode = .name
        }
        activePane.setSort(mode: mode)
    }

    @objc private func reverseChanged(_ sender: NSButton) {
        reverseSort = sender.state == .on
        activePane.setSortDescending(reverseSort)
    }

    @objc private func sortByName() {
        activePane.setSort(mode: .name)
    }

    @objc private func sortByType() {
        activePane.setSort(mode: .type)
    }

    @objc private func sortBySize() {
        activePane.setSort(mode: .size)
    }

    @objc private func toggleSortDirection() {
        reverseSort.toggle()
        activePane.setSortDescending(reverseSort)
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

        guard alert.runModal() == .alertFirstButtonReturn else {
            return nil
        }
        return field.stringValue
    }

    private func validName(_ name: String) -> Bool {
        !name.isEmpty && !name.contains("/")
    }

    @objc private func showPreferences() {
        let controller = PreferencesWindowController()
        preferencesController = controller
        controller.show()
    }

    @objc private func showAbout() {
        NSApp.orderFrontStandardAboutPanel(nil)
    }
}

final class SavedConnectionButton: NSButton {
    let profile: CPKSavedProfile

    init(
        title: String,
        profile: CPKSavedProfile,
        target: AnyObject?,
        action: Selector?
    ) {
        self.profile = profile
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
