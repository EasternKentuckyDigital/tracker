import SwiftUI

struct MenuBarTrackerView: View {
    @Environment(TrackerTheme.self) private var theme
    @Environment(\.openWindow) private var openWindow
    let store: TrackerStore

    @State private var taskName = ""

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            if let errorMessage = store.errorMessage {
                HStack(alignment: .top, spacing: 8) {
                    Image(systemName: "exclamationmark.triangle.fill")
                        .foregroundStyle(.orange)
                    Text(errorMessage)
                        .font(.caption)
                        .lineLimit(4)
                    Spacer(minLength: 4)
                    Button {
                        store.errorMessage = nil
                    } label: {
                        Image(systemName: "xmark")
                    }
                    .buttonStyle(.plain)
                    .accessibilityLabel("Dismiss error")
                }
                .padding(9)
                .background(.orange.opacity(0.1), in: RoundedRectangle(cornerRadius: 8))
            }

            if let active = store.snapshot.activeEntry {
                VStack(alignment: .leading, spacing: 5) {
                    Text(active.taskName)
                        .font(.headline)
                        .lineLimit(2)
                    TimelineView(.periodic(from: .now, by: 1)) { context in
                        Text(Duration.clock(active.elapsed(at: context.date)))
                            .font(.title2.bold().monospacedDigit())
                            .contentTransition(.numericText())
                    }
                    if let project = active.project {
                        Text(project)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                }

                Button {
                    Task { await store.stop(syncAfterChange: theme.syncAfterChanges) }
                } label: {
                    Label("Stop timer", systemImage: "stop.fill")
                        .frame(maxWidth: .infinity)
                }
                .buttonStyle(.borderedProminent)
                .tint(.red)
                .disabled(store.isWorking)
            } else {
                Text("Start tracking")
                    .font(.headline)
                TextField("What are you working on?", text: $taskName)
                    .textFieldStyle(.roundedBorder)
                    .onSubmit(start)
                    .onChange(of: taskName) {
                        taskName = taskName.limitedToUTF8Bytes(
                            TrackerInputLimits.taskNameBytes
                        )
                    }
                Button(action: start) {
                    Label("Start timer", systemImage: "play.fill")
                        .frame(maxWidth: .infinity)
                }
                .buttonStyle(.borderedProminent)
                .tint(theme.accent.color)
                .disabled(
                    store.isWorking
                        || taskName
                            .trimmingCharacters(in: .whitespacesAndNewlines)
                            .isEmpty
                )
            }

            Divider()

            HStack {
                Button {
                    Task { await store.sync() }
                } label: {
                    if store.isWorking {
                        ProgressView()
                            .controlSize(.small)
                    } else {
                        Label("Sync", systemImage: "arrow.triangle.2.circlepath")
                    }
                }
                .disabled(store.isWorking)

                Spacer()

                Button("Open Tracker") {
                    openWindow(id: "dashboard")
                }
            }

            SettingsLink {
                Label("Settings", systemImage: "gear")
            }
            .buttonStyle(.plain)
            .foregroundStyle(.secondary)
        }
        .padding(14)
        .frame(width: 290)
        .task {
            await store.refresh()
        }
    }

    private func start() {
        let name = taskName.trimmingCharacters(in: .whitespacesAndNewlines)
        Task {
            let started = await store.start(
                task: name,
                project: nil,
                tags: [],
                syncAfterChange: theme.syncAfterChanges
            )
            if started {
                taskName = ""
            }
        }
    }
}
