import Foundation

struct CPKEntry {
    let path: String
    let name: String
    let kind: String
    let size: UInt64?
    let modified: UInt64?

    var isDirectory: Bool { kind == "Directory" }
}

struct CPKSavedProfile {
    let id: String
    let name: String
    let host: String
    let username: String
    let port: UInt16
    let path: String

    var remote: SFTPConnection {
        SFTPConnection(host: host, username: username, port: port)
    }
}

struct SFTPConnection: Equatable {
    let host: String
    let username: String
    let port: UInt16

    var displayName: String {
        "\(username)@\(host):\(port)"
    }
}

enum BrowserConnection: Equatable {
    case local
    case sftp(SFTPConnection)

    var displayName: String {
        switch self {
        case .local:
            return "Local"
        case .sftp(let connection):
            return "SFTP — \(connection.displayName)"
        }
    }
}

enum CPKError: LocalizedError {
    case binaryNotFound
    case commandFailed(String)
    case unsupportedTransfer(String)

    var errorDescription: String? {
        switch self {
        case .binaryNotFound:
            return "Could not locate the cpk Rust companion binary."
        case .commandFailed(let message):
            return message
        case .unsupportedTransfer(let message):
            return message
        }
    }
}

final class CPKClient {
    private let binary: URL

    init() throws {
        if let explicit = ProcessInfo.processInfo.environment["CPK_BIN"],
           FileManager.default.isExecutableFile(atPath: explicit) {
            binary = URL(fileURLWithPath: explicit)
            return
        }

        let cwd = URL(fileURLWithPath: FileManager.default.currentDirectoryPath)
        let candidates = [
            cwd.appendingPathComponent("target/debug/cpk"),
            cwd.appendingPathComponent("../../../target/debug/cpk")
        ]
        guard let found = candidates.first(where: {
            FileManager.default.isExecutableFile(atPath: $0.path)
        }) else {
            throw CPKError.binaryNotFound
        }
        binary = found
    }

    func profiles() throws -> [CPKSavedProfile] {
        let data = try run(["profile-list"])
        guard let text = String(data: data, encoding: .utf8) else {
            throw CPKError.commandFailed("cpk returned non-UTF-8 profile data")
        }

        return text.split(separator: "\n").compactMap { line in
            let fields = line.split(
                separator: "\t",
                maxSplits: 5,
                omittingEmptySubsequences: false
            )
            guard fields.count == 6,
                  let port = UInt16(fields[4]) else {
                return nil
            }
            return CPKSavedProfile(
                id: String(fields[0]),
                name: String(fields[1]),
                host: String(fields[2]),
                username: String(fields[3]),
                port: port,
                path: String(fields[5])
            )
        }
    }

    func saveProfile(_ profile: CPKSavedProfile) throws {
        _ = try run([
            "profile-add",
            profile.id,
            profile.name,
            profile.host,
            profile.username,
            String(profile.port),
            profile.path
        ])
    }

    func removeProfile(id: String) throws {
        _ = try run(["profile-rm", id])
    }

    func list(connection: BrowserConnection, path: String) throws -> [CPKEntry] {
        let arguments: [String]
        switch connection {
        case .local:
            arguments = ["local-ls", path]
        case .sftp(let remote):
            arguments = [
                "sftp-ls",
                remote.host,
                remote.username,
                path,
                String(remote.port)
            ]
        }
        return try parseEntries(run(arguments))
    }

    func createDirectory(connection: BrowserConnection, path: String) throws {
        switch connection {
        case .local:
            _ = try run(["local-mkdir", path])
        case .sftp(let remote):
            _ = try run([
                "sftp-mkdir",
                remote.host,
                remote.username,
                path,
                String(remote.port)
            ])
        }
    }

    func rename(
        connection: BrowserConnection,
        source: String,
        destination: String
    ) throws {
        switch connection {
        case .local:
            _ = try run(["local-rename", source, destination])
        case .sftp(let remote):
            _ = try run([
                "sftp-rename",
                remote.host,
                remote.username,
                source,
                destination,
                String(remote.port)
            ])
        }
    }

    func remove(connection: BrowserConnection, path: String) throws {
        switch connection {
        case .local:
            _ = try run(["local-rm", path])
        case .sftp(let remote):
            _ = try run([
                "sftp-rm",
                remote.host,
                remote.username,
                path,
                String(remote.port)
            ])
        }
    }

    func copyTree(
        sourceConnection: BrowserConnection,
        source: String,
        destinationConnection: BrowserConnection,
        destination: String
    ) throws {
        switch (sourceConnection, destinationConnection) {
        case (.local, .local):
            _ = try run(["local-copy-tree-safe", source, destination])

        case (.local, .sftp(let remote)):
            _ = try run([
                "sftp-copy-tree-put",
                source,
                remote.host,
                remote.username,
                destination,
                String(remote.port)
            ])

        case (.sftp(let remote), .local):
            _ = try run([
                "sftp-copy-tree-get",
                remote.host,
                remote.username,
                source,
                destination,
                String(remote.port)
            ])

        case (.sftp, .sftp):
            throw CPKError.unsupportedTransfer(
                "SFTP-to-SFTP copy is not wired through the companion boundary yet."
            )
        }
    }

    private func parseEntries(_ data: Data) throws -> [CPKEntry] {
        guard let text = String(data: data, encoding: .utf8) else {
            throw CPKError.commandFailed("cpk returned non-UTF-8 output")
        }

        return text.split(separator: "\n").compactMap { line in
            let fields = line.split(
                separator: "\t",
                maxSplits: 3,
                omittingEmptySubsequences: false
            )
            guard fields.count == 4 else { return nil }

            let kind = String(fields[0])
            let size = fields[1] == "-" ? nil : UInt64(fields[1])
            let modified = fields[2] == "-" ? nil : UInt64(fields[2])
            let fullPath = String(fields[3])
            let name = URL(fileURLWithPath: fullPath).lastPathComponent

            return CPKEntry(
                path: fullPath,
                name: name.isEmpty ? fullPath : name,
                kind: kind,
                size: size,
                modified: modified
            )
        }
    }

    private func run(_ arguments: [String]) throws -> Data {
        let process = Process()
        process.executableURL = binary
        process.arguments = arguments

        let output = Pipe()
        let errors = Pipe()
        process.standardOutput = output
        process.standardError = errors

        try process.run()
        process.waitUntilExit()

        let data = output.fileHandleForReading.readDataToEndOfFile()
        if process.terminationStatus != 0 {
            let errorData = errors.fileHandleForReading.readDataToEndOfFile()
            let message = String(data: errorData, encoding: .utf8) ?? "cpk command failed"
            throw CPKError.commandFailed(
                message.trimmingCharacters(in: .whitespacesAndNewlines)
            )
        }
        return data
    }
}
