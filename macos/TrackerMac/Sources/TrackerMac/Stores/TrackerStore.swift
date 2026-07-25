import Foundation
import Observation

@MainActor
@Observable
final class TrackerStore {
    private let client: TrackerCLIClient
    private let securitySettings: TrackerSecuritySettings
    @ObservationIgnored
    private var periodicSyncTask: Task<Void, Never>?
    @ObservationIgnored
    private var operationInFlight = false
    @ObservationIgnored
    private var visibleOperationCount = 0

    var snapshot: TrackerSnapshot
    var selectedWeekStart: Date
    var isWorking = false
    var errorMessage: String?
    var lastSync: Date?

    init(
        client: TrackerCLIClient = TrackerCLIClient(),
        securitySettings: TrackerSecuritySettings? = nil,
        initialSnapshot: TrackerSnapshot = .empty,
        weekStart: Date = .now
    ) {
        self.client = client
        self.securitySettings = securitySettings ?? TrackerSecuritySettings()
        snapshot = initialSnapshot
        selectedWeekStart = weekStart
    }

    func selectCurrentWeek(using theme: TrackerTheme) {
        selectedWeekStart = theme.weekStart(containing: .now)
    }

    func moveWeek(by offset: Int) async {
        selectedWeekStart = Calendar.current.date(
            byAdding: .day,
            value: offset * 7,
            to: selectedWeekStart
        ) ?? selectedWeekStart
        await refresh()
    }

    func refresh() async {
        await perform(showProgress: snapshot.entries.isEmpty) {
            snapshot = try await client.snapshot(since: selectedWeekStart)
        }
    }

    func start(
        task: String,
        project: String?,
        tags: [String],
        syncAfterChange: Bool
    ) async -> Bool {
        guard !task.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
            errorMessage = "Give this timer a task name first."
            return false
        }
        let started = await perform {
            try await client.start(task: task, project: project, tags: tags)
            snapshot = try await client.snapshot(since: selectedWeekStart)
        }
        if started, syncAfterChange {
            await sync()
        }
        return started
    }

    func stop(syncAfterChange: Bool) async {
        let stopped = await perform {
            try await client.stop()
            snapshot = try await client.snapshot(since: selectedWeekStart)
        }
        if stopped, syncAfterChange {
            await sync()
        }
    }

    func sync(showProgress: Bool = true, reportError: Bool = true) async {
        await perform(showProgress: showProgress, reportError: reportError) {
            let configuration = securitySettings.configuration()
            try await client.sync(configuration: configuration)
            lastSync = .now
            snapshot = try await client.snapshot(since: selectedWeekStart)
        }
    }

    func configurePeriodicSync(enabled: Bool, minutes: Int) {
        periodicSyncTask?.cancel()
        periodicSyncTask = nil
        guard enabled else { return }

        let interval = max(1, minutes)
        periodicSyncTask = Task { [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(interval * 60))
                guard !Task.isCancelled else { return }
                await self?.sync(showProgress: false, reportError: false)
            }
        }
    }

    @discardableResult
    private func perform(
        showProgress: Bool = true,
        reportError: Bool = true,
        operation: () async throws -> Void
    ) async -> Bool {
        if showProgress {
            visibleOperationCount += 1
            isWorking = true
        }
        defer {
            if showProgress {
                visibleOperationCount -= 1
                isWorking = visibleOperationCount > 0
            }
        }
        while operationInFlight {
            do {
                try await Task.sleep(for: .milliseconds(50))
            } catch {
                return false
            }
        }
        operationInFlight = true
        defer { operationInFlight = false }
        do {
            try await operation()
            return true
        } catch is CancellationError {
            return false
        } catch {
            if reportError {
                errorMessage = error.localizedDescription
            }
            return false
        }
    }
}
