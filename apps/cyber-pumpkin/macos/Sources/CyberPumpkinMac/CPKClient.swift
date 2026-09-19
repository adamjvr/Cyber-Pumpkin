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

    var displayName: String { "\(username)@\(host):\(port)" }
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

    var isLocal: Bool {
        if case .local = self { return true }
        return false
    }
}

enum CPKCopyConflictPolicy: String {
    case fail
    case replace
    case skip
    case keepBoth = "keep-both"
}

enum CPKCopyResult {
    case completed(destination: String)
    case skipped(destination: String)
    case cancelled
}

struct CPKPreferences: Decodable {
    struct Files: Decodable {
        let confirmDelete: Bool
        let doubleClickAction: String
    }

    struct Transfers: Decodable {
        let downloadingFiles: String
        let downloadingFolders: String
        let uploadingFiles: String
        let uploadingFolders: String
        let simultaneousTransfers: UInt8
        let keepActivity: Bool
    }

    struct Advanced: Decodable {
        let keepConnectionsAlive: Bool
        let verboseLogging: Bool
    }

    let files: Files
    let transfers: Transfers
    let advanced: Advanced
}

enum CPKError: LocalizedError {
    case binaryNotFound
    case commandFailed(String)
    case malformedResponse(String)

    var errorDescription: String? {
        switch self {
        case .binaryNotFound:
            return "Could not locate the cpk Rust companion binary."
        case .commandFailed(let message), .malformedResponse(let message):
            return message
        }
    }
}

final class CPKClient {
    private struct TransferEndpointPayload: Encodable {
        let kind: String
        let path: String
        let host: String?
        let username: String?
        let port: UInt16?
    }

    private struct TransferTreePayload: Encodable {
        let operation_id: UInt64
        let source: TransferEndpointPayload
        let destination: TransferEndpointPayload
        let conflict_policy: String
    }

    private struct PreflightResponse: Decodable {
        let exists: Bool
    }

    private struct TransferResponse: Decodable {
        let state: String
        let destination: String?
    }

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
        let text = try runText(["profile-list"])
        return text.split(separator: "\n").compactMap { line in
            let fields = line.split(separator: "\t", maxSplits: 5, omittingEmptySubsequences: false)
            guard fields.count == 6, let port = UInt16(fields[4]) else { return nil }
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
            "profile-add", profile.id, profile.name, profile.host,
            profile.username, String(profile.port), profile.path
        ])
    }

    func removeProfile(id: String) throws {
        _ = try run(["profile-rm", id])
    }

    func preferences() throws -> CPKPreferences {
        let data = try run(["preferences-show"])
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        return try decoder.decode(CPKPreferences.self, from: data)
    }

    func setPreference(key: String, value: String) throws {
        _ = try run(["preferences-set", key, value])
    }

    func list(connection: BrowserConnection, path: String) throws -> [CPKEntry] {
        let arguments: [String]
        switch connection {
        case .local:
            arguments = ["local-ls", path]
        case .sftp(let remote):
            arguments = ["sftp-ls", remote.host, remote.username, path, String(remote.port)]
        }
        return try parseEntries(run(arguments))
    }

    func createDirectory(connection: BrowserConnection, path: String) throws {
        switch connection {
        case .local:
            _ = try run(["local-mkdir", path])
        case .sftp(let remote):
            _ = try run(["sftp-mkdir", remote.host, remote.username, path, String(remote.port)])
        }
    }

    func rename(connection: BrowserConnection, source: String, destination: String) throws {
        switch connection {
        case .local:
            _ = try run(["local-rename", source, destination])
        case .sftp(let remote):
            _ = try run([
                "sftp-rename", remote.host, remote.username,
                source, destination, String(remote.port)
            ])
        }
    }

    func remove(connection: BrowserConnection, path: String, recursive: Bool) throws {
        switch connection {
        case .local:
            _ = try run([recursive ? "local-rm-tree" : "local-rm", path])
        case .sftp(let remote):
            _ = try run([
                recursive ? "sftp-rm-tree" : "sftp-rm",
                remote.host, remote.username, path, String(remote.port)
            ])
        }
    }

    func destinationExists(
        sourceConnection: BrowserConnection,
        source: String,
        destinationConnection: BrowserConnection,
        destination: String
    ) throws -> Bool {
        let payload = transferPayload(
            sourceConnection: sourceConnection,
            source: source,
            destinationConnection: destinationConnection,
            destination: destination,
            conflictPolicy: .fail
        )
        let data = try runRequest(command: "transfer-tree-preflight", payload: payload)
        return try JSONDecoder().decode(PreflightResponse.self, from: data).exists
    }

    func copyTree(
        sourceConnection: BrowserConnection,
        source: String,
        destinationConnection: BrowserConnection,
        destination: String,
        conflictPolicy: CPKCopyConflictPolicy
    ) throws -> CPKCopyResult {
        let payload = transferPayload(
            sourceConnection: sourceConnection,
            source: source,
            destinationConnection: destinationConnection,
            destination: destination,
            conflictPolicy: conflictPolicy
        )
        let data = try runRequest(command: "transfer-tree-request", payload: payload)
        let response = try JSONDecoder().decode(TransferResponse.self, from: data)
        switch response.state {
        case "completed":
            return .completed(destination: response.destination ?? destination)
        case "skipped":
            return .skipped(destination: response.destination ?? destination)
        case "cancelled":
            return .cancelled
        default:
            throw CPKError.malformedResponse("Unknown transfer state: \(response.state)")
        }
    }

    private func transferPayload(
        sourceConnection: BrowserConnection,
        source: String,
        destinationConnection: BrowserConnection,
        destination: String,
        conflictPolicy: CPKCopyConflictPolicy
    ) -> TransferTreePayload {
        TransferTreePayload(
            operation_id: UInt64.random(in: 1...UInt64.max),
            source: endpointPayload(connection: sourceConnection, path: source),
            destination: endpointPayload(connection: destinationConnection, path: destination),
            conflict_policy: conflictPolicy.rawValue
        )
    }

    private func endpointPayload(
        connection: BrowserConnection,
        path: String
    ) -> TransferEndpointPayload {
        switch connection {
        case .local:
            return TransferEndpointPayload(
                kind: "local", path: path, host: nil, username: nil, port: nil
            )
        case .sftp(let remote):
            return TransferEndpointPayload(
                kind: "sftp", path: path, host: remote.host,
                username: remote.username, port: remote.port
            )
        }
    }

    private func runRequest<T: Encodable>(command: String, payload: T) throws -> Data {
        let request = FileManager.default.temporaryDirectory
            .appendingPathComponent("cyber-pumpkin-\(UUID().uuidString).json")
        let data = try JSONEncoder().encode(payload)
        try data.write(to: request, options: .atomic)
        defer { try? FileManager.default.removeItem(at: request) }
        return try run([command, request.path])
    }

    private func parseEntries(_ data: Data) throws -> [CPKEntry] {
        guard let text = String(data: data, encoding: .utf8) else {
            throw CPKError.malformedResponse("cpk returned non-UTF-8 directory data")
        }

        return text.split(separator: "\n").compactMap { line in
            let fields = line.split(separator: "\t", maxSplits: 3, omittingEmptySubsequences: false)
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

    private func runText(_ arguments: [String]) throws -> String {
        let data = try run(arguments)
        guard let text = String(data: data, encoding: .utf8) else {
            throw CPKError.malformedResponse("cpk returned non-UTF-8 output")
        }
        return text.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private func run(_ arguments: [String]) throws -> Data {
        let process = Process()
        process.executableURL = binary
        process.arguments = arguments

        let directory = FileManager.default.temporaryDirectory
        let token = UUID().uuidString
        let outputURL = directory.appendingPathComponent("cyber-pumpkin-stdout-\(token)")
        let errorURL = directory.appendingPathComponent("cyber-pumpkin-stderr-\(token)")
        FileManager.default.createFile(atPath: outputURL.path, contents: nil)
        FileManager.default.createFile(atPath: errorURL.path, contents: nil)
        defer {
            try? FileManager.default.removeItem(at: outputURL)
            try? FileManager.default.removeItem(at: errorURL)
        }

        let output = try FileHandle(forWritingTo: outputURL)
        let errors = try FileHandle(forWritingTo: errorURL)
        process.standardOutput = output
        process.standardError = errors

        do {
            try process.run()
            process.waitUntilExit()
            try output.close()
            try errors.close()
        } catch {
            try? output.close()
            try? errors.close()
            throw error
        }

        let data = try Data(contentsOf: outputURL)
        if process.terminationStatus != 0 {
            let errorData = try Data(contentsOf: errorURL)
            let message = String(data: errorData, encoding: .utf8) ?? "cpk command failed"
            throw CPKError.commandFailed(message.trimmingCharacters(in: .whitespacesAndNewlines))
        }
        return data
    }
}
