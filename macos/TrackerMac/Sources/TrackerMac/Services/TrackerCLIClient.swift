import Foundation

enum TrackerCLIError: LocalizedError {
    case unavailable
    case failed(command: String, message: String)
    case invalidResponse(String)

    var errorDescription: String? {
        switch self {
        case .unavailable:
            "Tracker CLI could not be found. Install it with `cargo install --path .`, or set TRACKER_CLI_PATH."
        case let .failed(command, message):
            "`tracker \(command)` failed: \(message)"
        case let .invalidResponse(message):
            "Tracker returned data the app could not read: \(message)"
        }
    }
}

actor TrackerCLIClient {
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
        var arguments = ["start", task]
        if let project, !project.isEmpty {
            arguments += ["--project", project]
        }
        for tag in tags where !tag.isEmpty {
            arguments += ["--tag", tag]
        }
        _ = try run(arguments)
    }

    func stop() throws {
        _ = try run(["stop"])
    }

    func sync() throws {
        _ = try run(["sync"])
    }

    private func run(_ arguments: [String]) throws -> Data {
        let process = Process()
        let output = Pipe()
        let errors = Pipe()

        if let executable {
            process.executableURL = executable
            process.arguments = arguments
        } else {
            process.executableURL = URL(fileURLWithPath: "/usr/bin/env")
            process.arguments = ["tracker"] + arguments
        }
        process.standardOutput = output
        process.standardError = errors
        process.environment = ProcessInfo.processInfo.environment

        do {
            try process.run()
        } catch {
            throw TrackerCLIError.unavailable
        }

        let outputData = output.fileHandleForReading.readDataToEndOfFile()
        let errorData = errors.fileHandleForReading.readDataToEndOfFile()
        process.waitUntilExit()

        guard process.terminationStatus == 0 else {
            let message = String(data: errorData, encoding: .utf8)?
                .trimmingCharacters(in: .whitespacesAndNewlines)
            throw TrackerCLIError.failed(
                command: arguments.joined(separator: " "),
                message: message?.replacingOccurrences(of: "error: ", with: "") ?? "Unknown error"
            )
        }
        return outputData
    }

    private static func locateExecutable() -> URL? {
        let fileManager = FileManager.default
        let environment = ProcessInfo.processInfo.environment
        let candidates = [
            environment["TRACKER_CLI_PATH"],
            Bundle.main.url(forResource: "tracker", withExtension: nil)?.path,
            "/opt/homebrew/bin/tracker",
            "/usr/local/bin/tracker"
        ].compactMap { $0 }

        return candidates
            .first(where: { fileManager.isExecutableFile(atPath: $0) })
            .map { URL(fileURLWithPath: $0) }
    }
}
