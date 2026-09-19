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
        main.addItem(editMenuItem())
        main.addItem(viewMenuItem())
        main.addItem(goMenuItem())
        main.addItem(transferMenuItem())
        main.addItem(windowMenuItem())
        main.addItem(helpMenuItem())
        NSApp.mainMenu = main
    }

    private func appMenuItem() -> NSMenuItem {
        let item = NSMenuItem()
        item.title = "Cyber-Pumpkin"
        let menu = NSMenu(title: "Cyber-Pumpkin")

        addTargetedItem(
            menu,
            "About Cyber-Pumpkin",
            #selector(showAbout),
            "",
            []
        )
        menu.addItem(.separator())
        addTargetedItem(
            menu,
            "Preferences…",
            #selector(showPreferences),
            ",",
            [.command]
        )
        menu.addItem(.separator())

        let servicesItem = NSMenuItem(title: "Services", action: nil, keyEquivalent: "")
        let services = NSMenu(title: "Services")
        servicesItem.submenu = services
        menu.addItem(servicesItem)
        NSApp.servicesMenu = services

        menu.addItem(.separator())
        addResponderItem(
            menu,
            "Hide Cyber-Pumpkin",
            #selector(NSApplication.hide(_:)),
            "h",
            [.command],
            target: NSApp
        )
        addResponderItem(
            menu,
            "Hide Others",
            #selector(NSApplication.hideOtherApplications(_:)),
            "h",
            [.command, .option],
            target: NSApp
        )
        addResponderItem(
            menu,
            "Show All",
            #selector(NSApplication.unhideAllApplications(_:)),
            "",
            [],
            target: NSApp
        )
        menu.addItem(.separator())
        addResponderItem(
            menu,
            "Quit Cyber-Pumpkin",
            #selector(NSApplication.terminate(_:)),
            "q",
            [.command],
            target: NSApp
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
        menu.addItem(.separator())
        addResponderItem(
            menu,
            "Close Window",
            #selector(NSWindow.performClose(_:)),
            "w",
            [.command]
        )

        item.submenu = menu
        return item
    }

    private func editMenuItem() -> NSMenuItem {
        let item = NSMenuItem()
        item.title = "Edit"
        let menu = NSMenu(title: "Edit")

        addResponderItem(menu, "Cut", #selector(NSText.cut(_:)), "x", [.command])
        addResponderItem(menu, "Copy", #selector(NSText.copy(_:)), "c", [.command])
        addResponderItem(menu, "Paste", #selector(NSText.paste(_:)), "v", [.command])
        menu.addItem(.separator())
        addResponderItem(
            menu,
            "Select All",
            #selector(NSResponder.selectAll(_:)),
            "a",
            [.command]
        )
        menu.addItem(.separator())
        addTargetedItem(
            menu,
            "Preferences…",
            #selector(showPreferences),
            ",",
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

    private func windowMenuItem() -> NSMenuItem {
        let item = NSMenuItem()
        item.title = "Window"
        let menu = NSMenu(title: "Window")

        addResponderItem(
            menu,
            "Minimize",
            #selector(NSWindow.performMiniaturize(_:)),
            "m",
            [.command]
        )
        addResponderItem(
            menu,
            "Zoom",
            #selector(NSWindow.performZoom(_:)),
            "",
            []
        )
        menu.addItem(.separator())
        addResponderItem(
            menu,
            "Bring All to Front",
            #selector(NSApplication.arrangeInFront(_:)),
            "",
            [],
            target: NSApp
        )

        item.submenu = menu
        NSApp.windowsMenu = menu
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

    private func addResponderItem(
        _ menu: NSMenu,
        _ title: String,
        _ action: Selector,
        _ keyEquivalent: String,
        _ modifiers: NSEvent.ModifierFlags,
        target: AnyObject? = nil
    ) {
        let item = NSMenuItem(
            title: title,
            action: action,
            keyEquivalent: keyEquivalent
        )
        item.target = target
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
            let open = SavedConnectionButton(
                title: profile.name,
                profile: profile,
                target: self,
                action: #selector(openSavedConnection(_:))
            )
            open.bezelStyle = .inline
            open.toolTip = "\(profile.username)@\(profile.host):\(profile.port)"

            let remove = RemoveSavedConnectionButton(
                profileID: profile.id,
                target: self,
                action: #selector(removeSavedConnection(_:))
            )
            remove.bezelStyle = .inline
            remove.toolTip = "Remove \(profile.name) from Pumpkin Patch"

            let row = NSStackView(views: [open, remove])
            row.orientation = .horizontal
            row.spacing = 4
            savedConnections.addArrangedSubview(row)
        }
    }

    @objc private func removeSavedConnection(_ sender: RemoveSavedConnectionButton) {
        do {
            try client.removeProfile(id: sender.profileID)
            reloadSavedConnections()
            status.stringValue = "Removed saved connection"
        } catch {
            status.stringValue =
                "Could not remove saved connection: \(error.localizedDescription)"
        }
    }

    @objc private func openSavedConnection(_ sender: SavedConnectionButton) {
        status.stringValue = "Connecting \(sender.profile.name)…"
        activePane.connectSFTP(
            sender.profile.remote,
            path: sender.profile.path
        ) { [weak self] result in
            guard let self else { return }
            switch result {
            case .success:
                self.status.stringValue = "Connected \(sender.profile.name)"
            case .failure(let error):
                self.status.stringValue =
                    "Saved connection failed: \(error.localizedDescription)"
            }
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

        guard alert.runModal() == .alertFirstButtonReturn else { return }
        guard let portNumber = UInt16(port.stringValue),
              portNumber > 0,
              !host.stringValue.isEmpty,
              !username.stringValue.isEmpty else {
            status.stringValue = "Invalid SFTP connection settings."
            return
        }

        let hostValue = host.stringValue
        let usernameValue = username.stringValue
        let pathValue = path.stringValue
        let displayValue = displayName.stringValue
        let shouldSave = saveProfile.state == .on
        let remote = SFTPConnection(
            host: hostValue,
            username: usernameValue,
            port: portNumber
        )

        status.stringValue = "Connecting \(remote.displayName)…"
        activePane.connectSFTP(remote, path: pathValue) { [weak self] result in
            guard let self else { return }
            switch result {
            case .success:
                if shouldSave {
                    let name = displayValue.isEmpty ? hostValue : displayValue
                    let profile = CPKSavedProfile(
                        id: self.profileID(
                            name: name,
                            host: hostValue,
                            username: usernameValue
                        ),
                        name: name,
                        host: hostValue,
                        username: usernameValue,
                        port: portNumber,
                        path: pathValue
                    )
                    do {
                        try self.client.saveProfile(profile)
                        self.reloadSavedConnections()
                    } catch {
                        self.status.stringValue =
                            "Connected, but save failed: \(error.localizedDescription)"
                        return
                    }
                }
                self.status.stringValue = "Connected \(remote.displayName)"
            case .failure(let error):
                self.status.stringValue =
                    "SFTP connection failed: \(error.localizedDescription)"
            }
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
        ), validName(name) else { return }

        let pane = activePane
        let connection = pane.connection
        let path = URL(fileURLWithPath: pane.currentPath)
            .appendingPathComponent(name)
            .path
        status.stringValue = "Creating \(name)…"

        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            guard let self else { return }
            let result = Result {
                try self.client.createDirectory(connection: connection, path: path)
            }
            DispatchQueue.main.async {
                switch result {
                case .success:
                    pane.reloadDirectory()
                    self.status.stringValue = "Created \(name)"
                case .failure(let error):
                    self.status.stringValue =
                        "Create failed: \(error.localizedDescription)"
                }
            }
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
        ), validName(name) else { return }

        let pane = activePane
        let connection = pane.connection
        let destination = URL(fileURLWithPath: pane.currentPath)
            .appendingPathComponent(name)
            .path
        status.stringValue = "Renaming \(entry.name)…"

        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            guard let self else { return }
            let result = Result {
                try self.client.rename(
                    connection: connection,
                    source: entry.path,
                    destination: destination
                )
            }
            DispatchQueue.main.async {
                switch result {
                case .success:
                    pane.reloadDirectory()
                    self.status.stringValue = "Renamed \(entry.name) → \(name)"
                case .failure(let error):
                    self.status.stringValue =
                        "Rename failed: \(error.localizedDescription)"
                }
            }
        }
    }

    @objc private func deleteSelected() {
        guard let entry = activePane.selectedEntry() else {
            status.stringValue = "Select an item to delete."
            return
        }

        let confirm = (try? client.preferences().files.confirmDelete) ?? true
        if confirm {
            let alert = NSAlert()
            alert.messageText = "Delete \(entry.name)?"
            alert.informativeText = entry.isDirectory
                ? "The folder and all of its contents will be removed."
                : "The selected file will be removed."
            alert.addButton(withTitle: "Delete")
            alert.addButton(withTitle: "Cancel")
            guard alert.runModal() == .alertFirstButtonReturn else { return }
        }

        let pane = activePane
        let connection = pane.connection
        status.stringValue = "Deleting \(entry.name)…"
        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            guard let self else { return }
            let result = Result {
                try self.client.remove(
                    connection: connection,
                    path: entry.path,
                    recursive: entry.isDirectory
                )
            }
            DispatchQueue.main.async {
                switch result {
                case .success:
                    pane.reloadDirectory()
                    self.status.stringValue = "Deleted \(entry.name)"
                    self.activity.stringValue = "Completed • Delete \(entry.name)"
                case .failure(let error):
                    self.status.stringValue = "Delete failed: \(error.localizedDescription)"
                    self.activity.stringValue = "Failed • Delete \(entry.name)"
                }
            }
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
        let sourceConnection = source.connection
        let destinationConnection = destination.connection
        status.stringValue = "Checking destination for \(entry.name)…"

        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            guard let self else { return }
            let result = Result {
                let exists = try self.client.destinationExists(
                    sourceConnection: sourceConnection,
                    source: entry.path,
                    destinationConnection: destinationConnection,
                    destination: destinationPath
                )
                let preferences = try? self.client.preferences()
                return (exists, preferences)
            }

            DispatchQueue.main.async {
                switch result {
                case .success(let (exists, preferences)):
                    self.resolveCopyPreflight(
                        source: source,
                        destination: destination,
                        entry: entry,
                        destinationPath: destinationPath,
                        destinationExists: exists,
                        preferences: preferences
                    )
                case .failure(let error):
                    self.status.stringValue =
                        "Destination preflight failed: \(error.localizedDescription)"
                }
            }
        }
    }

    private func resolveCopyPreflight(
        source: PaneViewController,
        destination: PaneViewController,
        entry: CPKEntry,
        destinationPath: String,
        destinationExists: Bool,
        preferences: CPKPreferences?
    ) {
        guard destinationExists else {
            performCopy(
                source: source,
                destination: destination,
                entry: entry,
                destinationPath: destinationPath,
                policy: .fail
            )
            return
        }

        let action = preferredExistingAction(
            source: source,
            destination: destination,
            entry: entry,
            preferences: preferences
        )
        switch action {
        case "Replace":
            performCopy(
                source: source,
                destination: destination,
                entry: entry,
                destinationPath: destinationPath,
                policy: .replace
            )
        case "Skip":
            status.stringValue = "Skipped existing \(entry.name)"
            activity.stringValue = "Completed • Skipped \(entry.name)"
        default:
            guard let policy = askConflictPolicy(name: entry.name) else {
                status.stringValue = "Copy cancelled before execution."
                return
            }
            performCopy(
                source: source,
                destination: destination,
                entry: entry,
                destinationPath: destinationPath,
                policy: policy
            )
        }
    }

    private func preferredExistingAction(
        source: PaneViewController,
        destination: PaneViewController,
        entry: CPKEntry,
        preferences: CPKPreferences?
    ) -> String {
        guard let preferences else { return "Ask" }
        let sourceLocal = source.connection.isLocal
        let destinationLocal = destination.connection.isLocal
        if sourceLocal == destinationLocal {
            return "Ask"
        }
        if !sourceLocal && destinationLocal {
            return entry.isDirectory
                ? preferences.transfers.downloadingFolders
                : preferences.transfers.downloadingFiles
        }
        return entry.isDirectory
            ? preferences.transfers.uploadingFolders
            : preferences.transfers.uploadingFiles
    }

    private func askConflictPolicy(name: String) -> CPKCopyConflictPolicy? {
        let alert = NSAlert()
        alert.messageText = "Destination Already Exists"
        alert.informativeText = "The destination already contains \(name)."
        alert.addButton(withTitle: "Replace")
        alert.addButton(withTitle: "Skip")
        alert.addButton(withTitle: "Keep Both")
        alert.addButton(withTitle: "Cancel")

        switch alert.runModal() {
        case .alertFirstButtonReturn:
            return .replace
        case .alertSecondButtonReturn:
            return .skip
        case .alertThirdButtonReturn:
            return .keepBoth
        default:
            return nil
        }
    }

    private func performCopy(
        source: PaneViewController,
        destination: PaneViewController,
        entry: CPKEntry,
        destinationPath: String,
        policy: CPKCopyConflictPolicy
    ) {
        status.stringValue = "Copying \(entry.name)…"
        let sourceConnection = source.connection
        let destinationConnection = destination.connection

        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            guard let self else { return }
            let result = Result {
                try self.client.copyTree(
                    sourceConnection: sourceConnection,
                    source: entry.path,
                    destinationConnection: destinationConnection,
                    destination: destinationPath,
                    conflictPolicy: policy
                )
            }

            DispatchQueue.main.async {
                switch result {
                case .success(.completed(let actualDestination)):
                    self.status.stringValue = "Copied \(entry.name)"
                    self.activity.stringValue =
                        "Completed • Copy \(entry.name) → \(actualDestination)"
                    destination.reloadDirectory()
                case .success(.skipped):
                    self.status.stringValue = "Skipped existing \(entry.name)"
                    self.activity.stringValue = "Completed • Skipped \(entry.name)"
                case .success(.cancelled):
                    self.status.stringValue = "Cancelled copy of \(entry.name)"
                    self.activity.stringValue = "Cancelled • Copy \(entry.name)"
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
        let controller = PreferencesWindowController(client: client)
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

final class RemoveSavedConnectionButton: NSButton {
    let profileID: String

    init(profileID: String, target: AnyObject?, action: Selector?) {
        self.profileID = profileID
        super.init(frame: .zero)
        title = "−"
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
