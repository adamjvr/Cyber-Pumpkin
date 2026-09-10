import Foundation

struct CPKEntry {
    let path: String
    let name: String
    let kind: String
    let size: UInt64?

    var isDirectory: Bool { kind == "Directory" }
}

enum CPKError: LocalizedError {
    case binaryNotFound
    case commandFailed(String)

    var errorDescription: String? {
        switch self {
        case .binaryNotFound:
            return "Could not locate the cpk Rust companion binary."
        case .commandFailed(let message):
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

    func list(path: String) throws -> [CPKEntry] {
        let data = try run(["local-ls", path])
        guard let text = String(data: data, encoding: .utf8) else {
            throw CPKError.commandFailed("cpk returned non-UTF-8 output")
        }

        return text.split(separator: "\n").compactMap { line in
            let fields = line.split(separator: "\t", maxSplits: 2, omittingEmptySubsequences: false)
            guard fields.count == 3 else { return nil }

            let kind = String(fields[0])
            let size = fields[1] == "-" ? nil : UInt64(fields[1])
            let fullPath = String(fields[2])
            let name = URL(fileURLWithPath: fullPath).lastPathComponent

            return CPKEntry(
                path: fullPath,
                name: name.isEmpty ? fullPath : name,
                kind: kind,
                size: size
            )
        }
    }


    func createDirectory(path: String) throws {
        _ = try run(["local-mkdir", path])
    }

    func rename(source: String, destination: String) throws {
        _ = try run(["local-rename", source, destination])
    }

    func remove(path: String) throws {
        _ = try run(["local-rm", path])
    }

    func copySafe(source: String, destination: String) throws {
        _ = try run(["local-copy-safe", source, destination])
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
            throw CPKError.commandFailed(message.trimmingCharacters(in: .whitespacesAndNewlines))
        }
        return data
    }
}
