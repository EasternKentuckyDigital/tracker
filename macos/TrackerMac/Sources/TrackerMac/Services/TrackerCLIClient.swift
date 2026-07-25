import Foundation
import Darwin

enum TrackerCLIError: LocalizedError {
    case unavailable
    case failed(command: String, message: String)
    case invalidResponse(String)
    case timedOut(command: String)
    case outputTooLarge(command: String)

    var errorDescription: String? {
        switch self {
        case .unavailable:
            "Tracker’s signed helper could not be found. Reinstall the app. Debug builds can set TRACKER_CLI_PATH."
        case let .failed(command, message):
            "`tracker \(command)` failed: \(message)"
        case let .invalidResponse(message):
            "Tracker returned data the app could not read: \(message)"
        case let .timedOut(command):
            "`tracker \(command)` did not finish in time."
        case let .outputTooLarge(command):
            "`tracker \(command)` returned more data than the app can safely process."
        }
    }
}

actor TrackerCLIClient {
    private static let maximumOutputBytes = 20 * 1024 * 1024
    private static let maximumErrorBytes = 128 * 1024
    private let executable: URL?

    init(executable: URL? = nil) {
        self.executable = executable ?? TrackerCLIClient.locateExecutable()
    }

    func snapshot(since: Date) throws -> TrackerSnapshot {
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        let data = try run(["snapshot", "--since", formatter.string(from: since)])
        do {
            let snapshot = try JSONDecoder.tracker.decode(TrackerSnapshot.self, from: data)
            guard snapshot.schemaVersion == 1 else {
                throw TrackerCLIError.invalidResponse(
                    "Unsupported snapshot version \(snapshot.schemaVersion)"
                )
            }
            return snapshot
        } catch {
            if let trackerError = error as? TrackerCLIError {
                throw trackerError
            }
            throw TrackerCLIError.invalidResponse(error.localizedDescription)
        }
    }

    func start(task: String, project: String?, tags: [String]) throws {
        var arguments = ["start"]
        if let project, !project.isEmpty {
            arguments += ["--project", project]
        }
        for tag in tags where !tag.isEmpty {
            arguments += ["--tag", tag]
        }
        arguments += ["--", task]
        _ = try run(arguments)
    }

    func stop() throws {
        _ = try run(["stop"])
    }

    func sync(configuration: TrackerSyncConfiguration) throws {
        var arguments = ["sync"]
        if let peerURL = configuration.peerURL {
            arguments += ["--peer", peerURL]
        }
        var environment: [String: String] = [:]
        if let token = configuration.token {
            environment["TRACKER_SYNC_TOKEN"] = token
        }
        _ = try run(arguments, environment: environment, timeout: 120)
    }

    private func run(
        _ arguments: [String],
        environment overrides: [String: String] = [:],
        timeout: TimeInterval = 15
    ) throws -> Data {
        guard let executable else {
            throw TrackerCLIError.unavailable
        }

        let process = Process()
        let output = Pipe()
        let errors = Pipe()
        let outputReader = BoundedPipeReader(limit: Self.maximumOutputBytes)
        let errorReader = BoundedPipeReader(limit: Self.maximumErrorBytes)

        process.executableURL = executable
        process.arguments = arguments
        process.standardOutput = output
        process.standardError = errors
        process.environment = Self.childEnvironment.merging(overrides) { _, override in override }

        do {
            try process.run()
        } catch {
            throw TrackerCLIError.unavailable
        }

        let readers = DispatchGroup()
        readers.enter()
        DispatchQueue.global(qos: .userInitiated).async {
            outputReader.drain(output.fileHandleForReading)
            readers.leave()
        }
        readers.enter()
        DispatchQueue.global(qos: .userInitiated).async {
            errorReader.drain(errors.fileHandleForReading)
            readers.leave()
        }

        let deadline = Date().addingTimeInterval(timeout)
        var timedOut = false
        while process.isRunning {
            if outputReader.didOverflow || errorReader.didOverflow {
                kill(process.processIdentifier, SIGTERM)
                break
            }
            if Date.now >= deadline {
                timedOut = true
                kill(process.processIdentifier, SIGTERM)
                break
            }
            Thread.sleep(forTimeInterval: 0.02)
        }
        if process.isRunning {
            Thread.sleep(forTimeInterval: 0.2)
        }
        if process.isRunning {
            kill(process.processIdentifier, SIGKILL)
        }
        process.waitUntilExit()
        let readersFinished = readers.wait(timeout: .now() + 1) == .success
        if !readersFinished {
            output.fileHandleForReading.closeFile()
            errors.fileHandleForReading.closeFile()
        }

        if timedOut || !readersFinished {
            throw TrackerCLIError.timedOut(command: safeCommand(arguments))
        }
        if outputReader.didOverflow || errorReader.didOverflow {
            throw TrackerCLIError.outputTooLarge(command: safeCommand(arguments))
        }

        guard process.terminationStatus == 0 else {
            let message = sanitizedMessage(errorReader.data)
            throw TrackerCLIError.failed(
                command: safeCommand(arguments),
                message: message.replacingOccurrences(of: "error: ", with: "")
            )
        }
        return outputReader.data
    }

    private static func locateExecutable() -> URL? {
        let fileManager = FileManager.default
        var candidates: [String] = [
            Bundle.main.bundleURL
                .appendingPathComponent("Contents/MacOS/tracker", isDirectory: false)
                .path
        ]
        if let resourceURL = Bundle.main.url(forResource: "tracker", withExtension: nil) {
            candidates.insert(resourceURL.path, at: 1)
        }

#if DEBUG
        candidates.append(contentsOf: [
            "/opt/homebrew/bin/tracker",
            "/usr/local/bin/tracker"
        ])
        if let override = ProcessInfo.processInfo.environment["TRACKER_CLI_PATH"] {
            candidates.insert(override, at: 0)
        }
#endif

        return candidates
            .first(where: { fileManager.isExecutableFile(atPath: $0) })
            .map {
                URL(fileURLWithPath: $0)
                    .standardizedFileURL
                    .resolvingSymlinksInPath()
            }
    }

    private static var childEnvironment: [String: String] {
        let parent = ProcessInfo.processInfo.environment
        let allowedKeys = ["HOME", "TMPDIR", "USER", "LOGNAME", "LANG", "LC_ALL"]
        var environment = allowedKeys.reduce(into: [String: String]()) { result, key in
            result[key] = parent[key]
        }
        environment["PATH"] =
            "/usr/bin:/bin:/usr/sbin:/sbin:/opt/homebrew/bin:/usr/local/bin"
#if DEBUG
        environment["TRACKER_DATABASE"] = parent["TRACKER_DATABASE"]
        environment["RUST_BACKTRACE"] = parent["RUST_BACKTRACE"]
#endif
        return environment
    }
}

private final class BoundedPipeReader: @unchecked Sendable {
    private let limit: Int
    private let lock = NSLock()
    private var storage = Data()
    private var overflow = false

    init(limit: Int) {
        self.limit = limit
    }

    var data: Data {
        lock.withLock { storage }
    }

    var didOverflow: Bool {
        lock.withLock { overflow }
    }

    func drain(_ handle: FileHandle) {
        while true {
            let chunk = handle.availableData
            if chunk.isEmpty {
                return
            }
            lock.withLock {
                let remaining = max(0, limit - storage.count)
                if chunk.count > remaining {
                    storage.append(contentsOf: chunk.prefix(remaining))
                    overflow = true
                } else {
                    storage.append(chunk)
                }
            }
        }
    }
}

private func safeCommand(_ arguments: [String]) -> String {
    arguments.first ?? "command"
}

private func sanitizedMessage(_ data: Data) -> String {
    guard let raw = String(data: data, encoding: .utf8) else {
        return "Unknown error"
    }
    let scalars = raw.unicodeScalars.map { scalar -> Character in
        let isAllowedWhitespace = CharacterSet.whitespacesAndNewlines.contains(scalar)
        let isControl = CharacterSet.controlCharacters.contains(scalar)
        return isAllowedWhitespace || !isControl ? Character(String(scalar)) : "�"
    }
    let message = String(scalars)
        .trimmingCharacters(in: .whitespacesAndNewlines)
    return message.isEmpty ? "Unknown error" : message
}
