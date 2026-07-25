import Foundation

struct TrackerSnapshot: Decodable, Sendable {
    let schemaVersion: Int
    let generatedAt: Date
    let activeEntry: TrackerEntry?
    let tasks: [TrackerTask]
    let entries: [TrackerEntry]

    static let empty = TrackerSnapshot(
        schemaVersion: 1,
        generatedAt: .now,
        activeEntry: nil,
        tasks: [],
        entries: []
    )

    static let preview: TrackerSnapshot = {
        let calendar = Calendar.current
        let today = calendar.startOfDay(for: .now)
        let chess = TrackerTask(
            id: "chess",
            name: "Chess Study",
            project: "Personal",
            tags: ["chess", "study"]
        )
        let paper = TrackerTask(
            id: "paper",
            name: "Read Bass Number Paper",
            project: "Cornell",
            tags: ["cornell", "reading"]
        )
        let entries = [
            TrackerEntry(
                id: "one",
                taskId: chess.id,
                taskName: chess.name,
                project: chess.project,
                tags: chess.tags,
                startedAt: calendar.date(byAdding: .hour, value: 9, to: today)!,
                stoppedAt: calendar.date(byAdding: .minute, value: 105, to: calendar.date(byAdding: .hour, value: 9, to: today)!)!
            ),
            TrackerEntry(
                id: "two",
                taskId: paper.id,
                taskName: paper.name,
                project: paper.project,
                tags: paper.tags,
                startedAt: calendar.date(byAdding: .day, value: -1, to: calendar.date(byAdding: .hour, value: 13, to: today)!)!,
                stoppedAt: calendar.date(byAdding: .day, value: -1, to: calendar.date(byAdding: .hour, value: 14, to: today)!)!
            )
        ]
        return TrackerSnapshot(
            schemaVersion: 1,
            generatedAt: .now,
            activeEntry: nil,
            tasks: [chess, paper],
            entries: entries
        )
    }()
}

struct TrackerTask: Decodable, Identifiable, Hashable, Sendable {
    let id: String
    let name: String
    let project: String?
    let tags: [String]

    private enum CodingKeys: String, CodingKey {
        case id, name, project, tags
    }
}

struct TrackerEntry: Decodable, Identifiable, Hashable, Sendable {
    let id: String
    let taskId: String
    let taskName: String
    let project: String?
    let tags: [String]
    let startedAt: Date
    let stoppedAt: Date?

    init(
        id: String,
        taskId: String,
        taskName: String,
        project: String?,
        tags: [String],
        startedAt: Date,
        stoppedAt: Date?
    ) {
        self.id = id
        self.taskId = taskId
        self.taskName = taskName
        self.project = project
        self.tags = tags
        self.startedAt = startedAt
        self.stoppedAt = stoppedAt
    }

    func elapsed(at date: Date = .now) -> TimeInterval {
        max(0, (stoppedAt ?? date).timeIntervalSince(startedAt))
    }

    private enum CodingKeys: String, CodingKey {
        case id, taskId, taskName, project, tags, startedAt, stoppedAt
    }
}

extension JSONDecoder {
    static var tracker: JSONDecoder {
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        decoder.dateDecodingStrategy = .custom { decoder in
            let value = try decoder.singleValueContainer().decode(String.self)
            let fractional = ISO8601DateFormatter()
            fractional.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
            if let date = fractional.date(from: value) {
                return date
            }
            let standard = ISO8601DateFormatter()
            guard let date = standard.date(from: value) else {
                throw DecodingError.dataCorruptedError(
                    in: try decoder.singleValueContainer(),
                    debugDescription: "Invalid ISO 8601 date: \(value)"
                )
            }
            return date
        }
        return decoder
    }
}

extension Duration {
    static func clock(_ seconds: TimeInterval) -> String {
        let total = max(0, Int(seconds))
        let hours = total / 3_600
        let minutes = (total % 3_600) / 60
        let seconds = total % 60
        return String(format: "%02d:%02d:%02d", hours, minutes, seconds)
    }

    static func compact(_ seconds: TimeInterval) -> String {
        let totalMinutes = max(0, Int(seconds) / 60)
        let hours = totalMinutes / 60
        let minutes = totalMinutes % 60
        if hours == 0 {
            return "\(minutes)m"
        }
        return minutes == 0 ? "\(hours)h" : "\(hours)h \(minutes)m"
    }
}
