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
        VStack(alignment: .leading, spacing: 12) {
            header

            if let active = store.snapshot.activeEntry {
                ActiveTimerCard(entry: active) {
                    Task { await store.stop(syncAfterChange: theme.syncAfterChanges) }
                }
            } else {
                startForm
            }

            if !store.snapshot.tasks.isEmpty {
                VStack(alignment: .leading, spacing: 7) {
                    Text("Recent tasks")
                        .font(.caption.weight(.semibold))
                        .foregroundStyle(.secondary)

                    ScrollView(.horizontal) {
                        LazyHStack(spacing: 8) {
                            ForEach(store.snapshot.tasks.prefix(10)) { task in
                                TaskShortcut(task: task) {
                                    taskName = task.name
                                    project = task.project ?? ""
                                    tags = task.tags.joined(separator: ", ")
                                    focusedField = .task
                                }
                            }
                        }
                    }
                    .scrollIndicators(.hidden)
                }
            }
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 14)
        .background(theme.secondaryBackground.opacity(0.48))
    }

    private var header: some View {
        HStack {
            Circle()
                .fill(theme.accent.color)
                .frame(width: 9, height: 9)
                .shadow(color: theme.accent.color.opacity(0.5), radius: 4)
            Text(store.snapshot.activeEntry == nil ? "Ready to track" : "Timer running")
                .font(.caption.weight(.semibold))
            Spacer()
            syncFooter
        }
        .foregroundStyle(.secondary)
    }

    private var startForm: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(alignment: .bottom, spacing: 10) {
                VStack(alignment: .leading, spacing: 4) {
                    Text("What are you working on?")
                        .font(.caption.weight(.semibold))
                        .foregroundStyle(.secondary)

                    TextField("Task name", text: $taskName)
                        .textFieldStyle(.roundedBorder)
                        .controlSize(.large)
                        .font(.system(size: 16 * theme.fontScale, weight: .medium))
                        .focused($focusedField, equals: .task)
                        .onSubmit { start() }
                        .onChange(of: taskName) {
                            taskName = taskName.limitedToUTF8Bytes(
                                TrackerInputLimits.taskNameBytes
                            )
                        }
                        .accessibilityLabel("Task name")
                }
                .frame(maxWidth: .infinity)

                Button(action: start) {
                    Label("Start timer", systemImage: "play.fill")
                        .frame(minWidth: 104)
                }
                .buttonStyle(.borderedProminent)
                .controlSize(.large)
                .tint(theme.accent.color)
                .keyboardShortcut(.return, modifiers: [.command])
                .disabled(
                    store.isWorking
                        || taskName
                            .trimmingCharacters(in: .whitespacesAndNewlines)
                            .isEmpty
                )
            }

            HStack(alignment: .top, spacing: 10) {
                VStack(alignment: .leading, spacing: 4) {
                    Text("Project")
                        .font(.caption2.weight(.semibold))
                        .foregroundStyle(.secondary)
                    TextField("Optional project", text: $project)
                        .textFieldStyle(.roundedBorder)
                        .controlSize(.large)
                        .focused($focusedField, equals: .project)
                        .onChange(of: project) {
                            project = project.limitedToUTF8Bytes(
                                TrackerInputLimits.projectBytes
                            )
                        }
                        .accessibilityLabel("Project")
                }
                .frame(maxWidth: .infinity)

                VStack(alignment: .leading, spacing: 4) {
                    Text("Tags")
                        .font(.caption2.weight(.semibold))
                        .foregroundStyle(.secondary)
                    TextField("Comma separated", text: $tags)
                        .textFieldStyle(.roundedBorder)
                        .controlSize(.large)
                        .focused($focusedField, equals: .tags)
                        .onChange(of: tags) {
                            tags = tags.limitedToUTF8Bytes(4_128)
                        }
                        .accessibilityLabel("Tags, comma separated")
                }
                .frame(maxWidth: .infinity)
            }
        }
        .padding(14)
        .background(theme.primaryBackground, in: RoundedRectangle(cornerRadius: 14))
        .overlay {
            RoundedRectangle(cornerRadius: 14)
                .stroke(theme.subtleBorder)
        }
    }

    private var syncFooter: some View {
        HStack(spacing: 6) {
            Image(systemName: store.lastSync == nil ? "icloud.slash" : "checkmark.icloud")
                .foregroundStyle(store.lastSync == nil ? .secondary : theme.accent.color)
            Text(syncText)
                .lineLimit(1)
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
            .prefix(TrackerInputLimits.tagsPerRecord)
            .map { $0.limitedToUTF8Bytes(TrackerInputLimits.tagBytes) }
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
        HStack(spacing: 14) {
            ZStack {
                Circle()
                    .fill(theme.accent.color.opacity(0.12))
                    .frame(width: 46, height: 46)
                Image(systemName: "waveform.path")
                    .symbolEffect(.variableColor.iterative)
                    .foregroundStyle(theme.accent.color)
            }

            VStack(alignment: .leading, spacing: 3) {
                Text("TRACKING NOW")
                    .font(.caption2.weight(.bold))
                    .foregroundStyle(theme.accent.color)
                Text(entry.taskName)
                    .font(.system(size: 17 * theme.fontScale, weight: .semibold))
                    .lineLimit(1)
                if let project = entry.project {
                    Label(project, systemImage: "folder")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }

            Spacer(minLength: 12)

            TimelineView(.periodic(from: .now, by: 1)) { context in
                Text(Duration.clock(entry.elapsed(at: context.date)))
                    .font(.system(size: 25 * theme.fontScale, weight: .bold, design: .monospaced))
                    .contentTransition(.numericText())
                    .frame(minWidth: 126, alignment: .trailing)
            }

            Button(action: stop) {
                Label("Stop timer", systemImage: "stop.fill")
                    .frame(minWidth: 92)
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
            .padding(.horizontal, 10)
            .padding(.vertical, 8)
            .frame(width: 190, alignment: .leading)
            .background(theme.primaryBackground.opacity(0.72), in: RoundedRectangle(cornerRadius: 8))
            .overlay {
                RoundedRectangle(cornerRadius: 8)
                    .stroke(theme.subtleBorder)
            }
        }
        .buttonStyle(.plain)
        .accessibilityLabel("Use task \(task.name)")
    }

    private var color: Color {
        task.tags.isEmpty ? theme.accent.color : EntryColor.color(for: task.tags[0], fallback: theme.accent.color)
    }
}
