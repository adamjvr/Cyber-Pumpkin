import AppKit

final class PreferencesWindowController {
    private let window: NSWindow

    init() {
        let tabs = NSTabViewController()
        tabs.tabStyle = .toolbar

        tabs.addTabViewItem(
            tab(title: "General", view: generalView())
        )
        tabs.addTabViewItem(
            tab(title: "Files", view: filesView())
        )
        tabs.addTabViewItem(
            tab(title: "Transfers", view: transfersView())
        )
        tabs.addTabViewItem(
            tab(title: "Rules", view: rulesView())
        )
        tabs.addTabViewItem(
            tab(title: "Keys", view: keysView())
        )
        tabs.addTabViewItem(
            tab(title: "Advanced", view: advancedView())
        )

        window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 760, height: 520),
            styleMask: [.titled, .closable],
            backing: .buffered,
            defer: false
        )
        window.title = "Cyber-Pumpkin Preferences"
        window.contentViewController = tabs
        window.center()
    }

    func show() {
        window.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
    }

    private func tab(title: String, view: NSView) -> NSTabViewItem {
        let controller = NSViewController()
        controller.view = view
        let item = NSTabViewItem(viewController: controller)
        item.label = title
        return item
    }

    private func generalView() -> NSView {
        preferencePage(
            title: "General",
            controls: [
                label("Startup locations, tab behavior, terminal integration, and update policy will live here."),
                label("Presentation remains native AppKit while shared behavior remains in Rust.")
            ]
        )
    }

    private func filesView() -> NSView {
        let confirm = NSButton(
            checkboxWithTitle: "Ask before deleting items",
            target: nil,
            action: nil
        )
        confirm.state = .on

        let doubleClick = NSPopUpButton()
        doubleClick.addItems(withTitles: ["Open", "Transfer", "Inspect"])

        return preferencePage(
            title: "Files",
            controls: [
                confirm,
                row(label: "Double-click action", control: doubleClick),
                label("Custom editor mappings will follow the remote-edit workspace.")
            ]
        )
    }

    private func transfersView() -> NSView {
        let downloadFiles = conflictPopup()
        let downloadFolders = conflictPopup()
        let uploadFiles = conflictPopup()
        let uploadFolders = conflictPopup()

        let simultaneous = NSStepper()
        simultaneous.minValue = 1
        simultaneous.maxValue = 20
        simultaneous.integerValue = 5

        return preferencePage(
            title: "Transfers",
            controls: [
                row(label: "Downloading files", control: downloadFiles),
                row(label: "Downloading folders", control: downloadFolders),
                row(label: "Uploading files", control: uploadFiles),
                row(label: "Uploading folders", control: uploadFolders),
                row(label: "Simultaneous transfers", control: simultaneous),
                NSButton(
                    checkboxWithTitle: "Keep completed activity items",
                    target: nil,
                    action: nil
                )
            ]
        )
    }

    private func rulesView() -> NSView {
        preferencePage(
            title: "Rules",
            controls: [
                label("Reusable include/skip rules will be shared by transfer and sync planning."),
                label("Fields: name, extension, type, path"),
                label("Operators: is, contains, starts with")
            ]
        )
    }

    private func keysView() -> NSView {
        preferencePage(
            title: "Keys",
            controls: [
                label("SFTP currently uses your SSH agent and OpenSSH configuration."),
                label("Secret material is intentionally kept out of saved Pumpkin Patch profiles.")
            ]
        )
    }

    private func advancedView() -> NSView {
        let keepAlive = NSButton(
            checkboxWithTitle: "Keep idle connections alive",
            target: nil,
            action: nil
        )
        keepAlive.state = .on
        let verbose = NSButton(
            checkboxWithTitle: "Verbose logging",
            target: nil,
            action: nil
        )

        return preferencePage(
            title: "Advanced",
            controls: [
                keepAlive,
                verbose,
                label("Proxy and protocol-specific tuning will build on the shared Rust preferences model.")
            ]
        )
    }

    private func conflictPopup() -> NSPopUpButton {
        let popup = NSPopUpButton()
        popup.addItems(withTitles: ["Ask", "Replace", "Skip"])
        return popup
    }

    private func preferencePage(
        title: String,
        controls: [NSView]
    ) -> NSView {
        let heading = NSTextField(labelWithString: title)
        heading.font = .boldSystemFont(ofSize: 20)

        let stack = NSStackView()
        stack.orientation = .vertical
        stack.alignment = .leading
        stack.spacing = 14
        stack.edgeInsets = NSEdgeInsets(
            top: 28,
            left: 32,
            bottom: 28,
            right: 32
        )
        stack.addArrangedSubview(heading)

        for control in controls {
            stack.addArrangedSubview(control)
        }

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

    private func label(_ text: String) -> NSTextField {
        let field = NSTextField(wrappingLabelWithString: text)
        field.textColor = .secondaryLabelColor
        return field
    }
}
