import AppKit

final class PreferencesWindowController: NSObject {
    private let client: CPKClient
    private var window: NSWindow!

    private let confirmDelete = NSButton(
        checkboxWithTitle: "Ask before deleting items", target: nil, action: nil
    )
    private let downloadFiles = NSPopUpButton()
    private let downloadFolders = NSPopUpButton()
    private let uploadFiles = NSPopUpButton()
    private let uploadFolders = NSPopUpButton()
    private let keepAlive = NSButton(
        checkboxWithTitle: "Keep idle SFTP connections alive", target: nil, action: nil
    )

    init(client: CPKClient) {
        self.client = client
        super.init()
        configureControls()

        let tabs = NSTabViewController()
        tabs.tabStyle = .toolbar
        tabs.addTabViewItem(tab(title: "Files", view: filesView()))
        tabs.addTabViewItem(tab(title: "Transfers", view: transfersView()))
        tabs.addTabViewItem(tab(title: "Advanced", view: advancedView()))

        window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 720, height: 440),
            styleMask: [.titled, .closable],
            backing: .buffered,
            defer: false
        )
        window.title = "Cyber-Pumpkin Preferences"
        window.contentViewController = tabs
        window.center()
        loadPreferences()
    }

    func show() {
        window.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
    }

    private func configureControls() {
        for popup in [downloadFiles, downloadFolders, uploadFiles, uploadFolders] {
            popup.addItems(withTitles: ["Ask", "Replace", "Skip"])
            popup.target = self
            popup.action = #selector(conflictChanged(_:))
        }
        confirmDelete.target = self
        confirmDelete.action = #selector(confirmDeleteChanged(_:))
        keepAlive.target = self
        keepAlive.action = #selector(keepAliveChanged(_:))
    }

    private func loadPreferences() {
        guard let preferences = try? client.preferences() else { return }
        confirmDelete.state = preferences.files.confirmDelete ? .on : .off
        select(downloadFiles, value: preferences.transfers.downloadingFiles)
        select(downloadFolders, value: preferences.transfers.downloadingFolders)
        select(uploadFiles, value: preferences.transfers.uploadingFiles)
        select(uploadFolders, value: preferences.transfers.uploadingFolders)
        keepAlive.state = preferences.advanced.keepConnectionsAlive ? .on : .off
    }

    private func select(_ popup: NSPopUpButton, value: String) {
        popup.selectItem(withTitle: value)
    }

    @objc private func confirmDeleteChanged(_ sender: NSButton) {
        persist(
            key: "files.confirm-delete",
            value: sender.state == .on ? "true" : "false"
        )
    }

    @objc private func keepAliveChanged(_ sender: NSButton) {
        persist(
            key: "advanced.keep-connections-alive",
            value: sender.state == .on ? "true" : "false"
        )
    }

    @objc private func conflictChanged(_ sender: NSPopUpButton) {
        let key: String
        if sender === downloadFiles {
            key = "transfers.downloading-files"
        } else if sender === downloadFolders {
            key = "transfers.downloading-folders"
        } else if sender === uploadFiles {
            key = "transfers.uploading-files"
        } else {
            key = "transfers.uploading-folders"
        }
        persist(key: key, value: sender.titleOfSelectedItem?.lowercased() ?? "ask")
    }

    private func persist(key: String, value: String) {
        do {
            try client.setPreference(key: key, value: value)
        } catch {
            let alert = NSAlert()
            alert.messageText = "Could not save preferences"
            alert.informativeText = error.localizedDescription
            alert.runModal()
            loadPreferences()
        }
    }

    private func tab(title: String, view: NSView) -> NSTabViewItem {
        let controller = NSViewController()
        controller.view = view
        let item = NSTabViewItem(viewController: controller)
        item.label = title
        return item
    }

    private func filesView() -> NSView {
        preferencePage(
            title: "Files",
            controls: [
                confirmDelete,
                note("Deletion policy is shared with the Linux frontend through the Rust preferences model.")
            ]
        )
    }

    private func transfersView() -> NSView {
        preferencePage(
            title: "Transfers",
            controls: [
                row(label: "Downloading files", control: downloadFiles),
                row(label: "Downloading folders", control: downloadFolders),
                row(label: "Uploading files", control: uploadFiles),
                row(label: "Uploading folders", control: uploadFolders),
                note("Ask offers Replace, Skip, Keep Both, or Cancel when a destination already exists.")
            ]
        )
    }

    private func advancedView() -> NSView {
        preferencePage(
            title: "Advanced",
            controls: [
                keepAlive,
                note("SFTP host verification remains strict and uses OpenSSH known_hosts.")
            ]
        )
    }

    private func preferencePage(title: String, controls: [NSView]) -> NSView {
        let heading = NSTextField(labelWithString: title)
        heading.font = .boldSystemFont(ofSize: 20)
        let stack = NSStackView()
        stack.orientation = .vertical
        stack.alignment = .leading
        stack.spacing = 14
        stack.edgeInsets = NSEdgeInsets(top: 28, left: 32, bottom: 28, right: 32)
        stack.addArrangedSubview(heading)
        for control in controls { stack.addArrangedSubview(control) }
        return stack
    }

    private func row(label: String, control: NSView) -> NSView {
        let title = NSTextField(labelWithString: label)
        title.alignment = .right
        title.widthAnchor.constraint(equalToConstant: 180).isActive = true
        let stack = NSStackView(views: [title, control])
        stack.orientation = .horizontal
        stack.spacing = 12
        return stack
    }

    private func note(_ text: String) -> NSTextField {
        let field = NSTextField(wrappingLabelWithString: text)
        field.textColor = .secondaryLabelColor
        return field
    }
}
