import AppKit

enum PaneSortMode: String {
    case name
    case type
    case size
}

final class ContextTableView: NSTableView {
    override func menu(for event: NSEvent) -> NSMenu? {
        let point = convert(event.locationInWindow, from: nil)
        let clickedRow = row(at: point)
        if clickedRow >= 0 {
            selectRowIndexes(
                IndexSet(integer: clickedRow),
                byExtendingSelection: false
            )
        }
        return super.menu(for: event)
    }
}

final class PaneViewController: NSViewController, NSTableViewDataSource, NSTableViewDelegate {
    let titleText: String
    let client: CPKClient
    private(set) var currentPath: String

    var showHidden = false {
        didSet { applyFilterAndSort() }
    }

    var onBecameActive: (() -> Void)?

    private var allEntries: [CPKEntry] = []
    private var visibleEntries: [CPKEntry] = []
    private var sortMode: PaneSortMode = .name
    private var sortDescending = false
    private let pathField = NSTextField()
    private let tableView = ContextTableView()
    private let footer = NSTextField(labelWithString: "0 items")

    init(title: String, path: String, client: CPKClient) {
        titleText = title
        currentPath = path
        self.client = client
        super.init(nibName: nil, bundle: nil)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("init(coder:) is not supported")
    }

    override func loadView() {
        view = NSView()

        let title = NSTextField(labelWithString: titleText)
        title.font = .boldSystemFont(ofSize: NSFont.systemFontSize)

        let up = NSButton(title: "⌃", target: self, action: #selector(goUp))
        up.toolTip = "Up"

        pathField.stringValue = currentPath
        pathField.target = self
        pathField.action = #selector(pathCommitted)

        let nav = NSStackView(views: [up, pathField])
        nav.orientation = .horizontal
        nav.spacing = 4
        pathField.setContentHuggingPriority(.defaultLow, for: .horizontal)

        configureTable()

        let scroll = NSScrollView()
        scroll.documentView = tableView
        scroll.hasVerticalScroller = true
        scroll.borderType = .noBorder

        footer.textColor = .secondaryLabelColor

        let stack = NSStackView(views: [title, nav, scroll, footer])
        stack.orientation = .vertical
        stack.spacing = 6
        stack.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(stack)

        NSLayoutConstraint.activate([
            stack.leadingAnchor.constraint(equalTo: view.leadingAnchor, constant: 8),
            stack.trailingAnchor.constraint(equalTo: view.trailingAnchor, constant: -8),
            stack.topAnchor.constraint(equalTo: view.topAnchor, constant: 8),
            stack.bottomAnchor.constraint(equalTo: view.bottomAnchor, constant: -8),
            scroll.heightAnchor.constraint(greaterThanOrEqualToConstant: 300)
        ])

        reloadDirectory()
    }

    func reloadDirectory() {
        do {
            allEntries = try client.list(path: currentPath)
            applyFilterAndSort()
            pathField.stringValue = currentPath
        } catch {
            footer.stringValue = "Load failed: \(error.localizedDescription)"
        }
    }

    func navigate(to path: String) {
        currentPath = path
        reloadDirectory()
        onBecameActive?()
    }

    func selectedEntry() -> CPKEntry? {
        let selected = tableView.selectedRow
        guard selected >= 0, selected < visibleEntries.count else {
            return nil
        }
        return visibleEntries[selected]
    }

    func setSort(mode: PaneSortMode) {
        sortMode = mode
        applyFilterAndSort()
    }

    func setSortDescending(_ descending: Bool) {
        sortDescending = descending
        applyFilterAndSort()
    }

    func installContextMenu(_ menu: NSMenu) {
        tableView.menu = menu
    }

    private func applyFilterAndSort() {
        visibleEntries = showHidden
            ? allEntries
            : allEntries.filter { !$0.name.hasPrefix(".") }

        visibleEntries.sort(by: entryComesBefore)
        if sortDescending {
            visibleEntries.reverse()
        }

        tableView.reloadData()
        let order = sortDescending ? "descending" : "ascending"
        footer.stringValue =
            "\(visibleEntries.count) items • \(sortMode.rawValue) \(order)"
    }

    private func entryComesBefore(_ left: CPKEntry, _ right: CPKEntry) -> Bool {
        if left.isDirectory != right.isDirectory {
            return left.isDirectory
        }

        switch sortMode {
        case .name:
            return compareNames(left, right)
        case .type:
            if left.kind == right.kind {
                return compareNames(left, right)
            }
            return left.kind.localizedCaseInsensitiveCompare(right.kind) == .orderedAscending
        case .size:
            if left.size == right.size {
                return compareNames(left, right)
            }
            return (left.size ?? 0) < (right.size ?? 0)
        }
    }

    private func compareNames(_ left: CPKEntry, _ right: CPKEntry) -> Bool {
        left.name.localizedCaseInsensitiveCompare(right.name) == .orderedAscending
    }

    private func configureTable() {
        let columns: [(String, String, CGFloat)] = [
            ("name", "Name", 300),
            ("kind", "Type", 90),
            ("size", "Size", 100)
        ]
        for (identifier, title, width) in columns {
            let column = NSTableColumn(
                identifier: NSUserInterfaceItemIdentifier(identifier)
            )
            column.title = title
            column.width = width
            tableView.addTableColumn(column)
        }

        tableView.delegate = self
        tableView.dataSource = self
        tableView.target = self
        tableView.doubleAction = #selector(openSelected)
        tableView.allowsMultipleSelection = false
    }

    func numberOfRows(in tableView: NSTableView) -> Int {
        visibleEntries.count
    }

    func tableView(
        _ tableView: NSTableView,
        viewFor tableColumn: NSTableColumn?,
        row: Int
    ) -> NSView? {
        let entry = visibleEntries[row]
        let value: String

        switch tableColumn?.identifier.rawValue {
        case "kind":
            value = entry.kind
        case "size":
            value = formatSize(entry.size)
        default:
            value = entry.name
        }

        return NSTextField(labelWithString: value)
    }

    func tableViewSelectionDidChange(_ notification: Notification) {
        onBecameActive?()

        guard let entry = selectedEntry() else {
            updateFooter()
            return
        }

        footer.stringValue =
            "\(entry.name) selected • \(formatSize(entry.size))"
    }

    @objc private func pathCommitted() {
        navigate(to: pathField.stringValue)
    }

    @objc private func openSelected() {
        guard let entry = selectedEntry(), entry.isDirectory else {
            return
        }
        navigate(to: entry.path)
    }

    @objc private func goUp() {
        let parent = URL(fileURLWithPath: currentPath)
            .deletingLastPathComponent()
            .path
        if parent != currentPath {
            navigate(to: parent)
        }
    }

    private func updateFooter() {
        let order = sortDescending ? "descending" : "ascending"
        footer.stringValue =
            "\(visibleEntries.count) items • \(sortMode.rawValue) \(order)"
    }

    func formatSize(_ size: UInt64?) -> String {
        guard let bytes = size else {
            return "—"
        }
        if bytes < 1_024 {
            return "\(bytes) B"
        }
        if bytes < 1_048_576 {
            return "\(bytes / 1_024) KiB"
        }
        if bytes < 1_073_741_824 {
            return "\(bytes / 1_048_576) MiB"
        }
        return "\(bytes / 1_073_741_824) GiB"
    }
}
