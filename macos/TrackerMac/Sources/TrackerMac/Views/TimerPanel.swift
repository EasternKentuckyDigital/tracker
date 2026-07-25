import SwiftUI

struct TimerPanel: View {
    @Environment(TrackerTheme.self) private var theme
    let store: TrackerStore

    @State private var taskName = ""
    @State private var project = ""
    @State private var tags = ""
    @FocusState private var focusedField: Field?

    private enum Field {
        case task, project, tags
    }

    var body: some View {
        VStack(alignment: .leading, spacing: theme.density.spacing) {
            header

            if let active = store.snapshot.activeEntry {
                ActiveTimerCard(entry: active) {
                    Task { await store.stop(syncAfterChange: theme.syncAfterChanges) }
                }
            } else {
                startForm
            }

            Divider()

            Text("Recent tasks")
                .font(.headline)

            if store.snapshot.tasks.isEmpty {
                ContentUnavailableView(
                    "No tasks yet",
                    systemImage: "timer",
                    description: Text("Start a timer and it will appear here.")
                )
            } else {
                ScrollView {
                    LazyVStack(spacing: 6) {
                        ForEach(store.snapshot.tasks.prefix(12)) { task in
                            TaskShortcut(task: task) {
                                taskName = task.name
                                project = task.project ?? ""
                                tags = task.tags.joined(separator: ", ")
                                focusedField = .task
                            }
                        }
                    }
                }
            }

            syncFooter
        }
        .padding(theme.density == .compact ? 12 : 16)
        .background(theme.secondaryBackground.opacity(0.48))
    }

    private var header: some View {
        HStack {
            VStack(alignment: .leading, spacing: 2) {
                Text("Tracker")
                    .font(.system(size: 22 * theme.fontScale, weight: .bold, design: .rounded))
                Text("Your time, on your devices.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            Spacer()
            Circle()
                .fill(theme.accent.color)
                .frame(width: 10, height: 10)
                .shadow(color: theme.accent.color.opacity(0.5), radius: 5)
        }
    }

    private var startForm: some View {
        VStack(alignment: .leading, spacing: 8) {
            TextField("What are you working on?", text: $taskName)
                .textFieldStyle(.roundedBorder)
                .font(.system(size: 15 * theme.fontScale, weight: .medium))
                .focused($focusedField, equals: .task)
                .onSubmit { start() }
                .accessibilityLabel("Task name")

            HStack {
                TextField("Project (optional)", text: $project)
                    .textFieldStyle(.roundedBorder)
                    .focused($focusedField, equals: .project)
                TextField("Tags, comma separated", text: $tags)
                    .textFieldStyle(.roundedBorder)
                    .focused($focusedField, equals: .tags)
            }
            .font(.caption)

            Button(action: start) {
                Label("Start timer", systemImage: "play.fill")
                    .frame(maxWidth: .infinity)
            }
            .buttonStyle(.borderedProminent)
            .controlSize(.large)
            .tint(theme.accent.color)
            .keyboardShortcut(.return, modifiers: [.command])
            .disabled(store.isWorking || taskName.trimmingCharacters(in: .whitespaces).isEmpty)
        }
        .padding(12)
        .background(theme.primaryBackground, in: RoundedRectangle(cornerRadius: 12))
        .overlay {
            RoundedRectangle(cornerRadius: 12)
                .stroke(theme.subtleBorder)
        }
    }

    private var syncFooter: some View {
        HStack(spacing: 6) {
            Image(systemName: store.lastSync == nil ? "icloud.slash" : "checkmark.icloud")
                .foregroundStyle(store.lastSync == nil ? .secondary : theme.accent.color)
            Text(syncText)
                .lineLimit(1)
            Spacer()
            Button {
                Task { await store.sync() }
            } label: {
                Image(systemName: "arrow.clockwise")
            }
            .buttonStyle(.plain)
            .disabled(store.isWorking)
            .help("Sync now")
        }
        .font(.caption)
        .foregroundStyle(.secondary)
    }

    private var syncText: String {
        guard let date = store.lastSync else { return "Not synced this session" }
        return "Synced \(date.formatted(date: .omitted, time: .shortened))"
    }

    private func start() {
        let parsedTags = tags
            .split(separator: ",")
            .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
            .filter { !$0.isEmpty }
        Task {
            let started = await store.start(
                task: taskName.trimmingCharacters(in: .whitespacesAndNewlines),
                project: project.trimmingCharacters(in: .whitespacesAndNewlines),
                tags: parsedTags,
                syncAfterChange: theme.syncAfterChanges
            )
            if started {
                taskName = ""
                project = ""
                tags = ""
            }
        }
    }
}

private struct ActiveTimerCard: View {
    @Environment(TrackerTheme.self) private var theme
    let entry: TrackerEntry
    let stop: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack {
                Image(systemName: "waveform.path")
                    .symbolEffect(.variableColor.iterative)
                    .foregroundStyle(theme.accent.color)
                Text("Tracking now")
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(theme.accent.color)
                Spacer()
            }

            Text(entry.taskName)
                .font(.system(size: 17 * theme.fontScale, weight: .semibold))
                .lineLimit(2)

            TimelineView(.periodic(from: .now, by: 1)) { context in
                Text(Duration.clock(entry.elapsed(at: context.date)))
                    .font(.system(size: 28 * theme.fontScale, weight: .bold, design: .monospaced))
                    .contentTransition(.numericText())
            }

            if let project = entry.project {
                Label(project, systemImage: "folder")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }

            Button(action: stop) {
                Label("Stop timer", systemImage: "stop.fill")
                    .frame(maxWidth: .infinity)
            }
            .buttonStyle(.borderedProminent)
            .controlSize(.large)
            .tint(.red)
            .keyboardShortcut(".", modifiers: [.command])
        }
        .padding(14)
        .background(theme.accent.color.opacity(0.1), in: RoundedRectangle(cornerRadius: 14))
        .overlay {
            RoundedRectangle(cornerRadius: 14)
                .stroke(theme.accent.color.opacity(0.25))
        }
    }
}

private struct TaskShortcut: View {
    @Environment(TrackerTheme.self) private var theme
    let task: TrackerTask
    let select: () -> Void

    var body: some View {
        Button(action: select) {
            HStack(spacing: 9) {
                RoundedRectangle(cornerRadius: 3)
                    .fill(color)
                    .frame(width: 7, height: 28)

                VStack(alignment: .leading, spacing: 2) {
                    Text(task.name)
                        .lineLimit(1)
                        .foregroundStyle(.primary)
                    HStack(spacing: 5) {
                        if let project = task.project {
                            Text(project)
                        }
                        if let firstTag = task.tags.first {
                            Text("#\(firstTag)")
                        }
                    }
                    .font(.caption2)
                    .foregroundStyle(.secondary)
                }
                Spacer()
                Image(systemName: "arrow.up.left")
                    .font(.caption2)
                    .foregroundStyle(.tertiary)
            }
            .padding(7)
            .background(theme.primaryBackground.opacity(0.72), in: RoundedRectangle(cornerRadius: 8))
        }
        .buttonStyle(.plain)
        .accessibilityLabel("Use task \(task.name)")
    }

    private var color: Color {
        task.tags.isEmpty ? theme.accent.color : EntryColor.color(for: task.tags[0], fallback: theme.accent.color)
    }
}
