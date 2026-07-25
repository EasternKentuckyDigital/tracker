import SwiftUI

struct DashboardView: View {
    @Environment(TrackerTheme.self) private var theme
    let store: TrackerStore

    var body: some View {
        VStack(spacing: 0) {
            TimerPanel(store: store)

            Divider()

            WeekCalendarView(store: store)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
        .background(theme.primaryBackground)
        .toolbar {
            ToolbarItemGroup(placement: .navigation) {
                Button {
                    Task { await store.moveWeek(by: -1) }
                } label: {
                    Label("Previous Week", systemImage: "chevron.left")
                }

                Button("Today") {
                    store.selectCurrentWeek(using: theme)
                    Task { await store.refresh() }
                }

                Button {
                    Task { await store.moveWeek(by: 1) }
                } label: {
                    Label("Next Week", systemImage: "chevron.right")
                }
            }

            ToolbarItemGroup(placement: .primaryAction) {
                if store.isWorking {
                    ProgressView()
                        .controlSize(.small)
                }

                Button {
                    Task { await store.sync() }
                } label: {
                    Label("Sync", systemImage: "arrow.triangle.2.circlepath")
                }
                .disabled(store.isWorking)
                .help("Sync with Tracker devices on Tailscale")

                SettingsLink {
                    Label("Settings", systemImage: "slider.horizontal.3")
                }
            }
        }
        .task {
            store.selectCurrentWeek(using: theme)
            await store.refresh()
            store.configurePeriodicSync(
                enabled: theme.periodicSyncEnabled,
                minutes: theme.syncIntervalMinutes
            )
        }
        .onChange(of: theme.weekStartsMonday) {
            store.selectCurrentWeek(using: theme)
            Task { await store.refresh() }
        }
        .onChange(of: theme.periodicSyncEnabled) {
            store.configurePeriodicSync(
                enabled: theme.periodicSyncEnabled,
                minutes: theme.syncIntervalMinutes
            )
        }
        .onChange(of: theme.syncIntervalMinutes) {
            store.configurePeriodicSync(
                enabled: theme.periodicSyncEnabled,
                minutes: theme.syncIntervalMinutes
            )
        }
        .alert(
            "Tracker",
            isPresented: Binding(
                get: { store.errorMessage != nil },
                set: { if !$0 { store.errorMessage = nil } }
            )
        ) {
            Button("OK") { store.errorMessage = nil }
        } message: {
            Text(store.errorMessage ?? "")
        }
    }
}

#Preview("Dashboard") {
    let theme = TrackerTheme()
    let store = TrackerStore(
        initialSnapshot: .preview,
        weekStart: theme.weekStart(containing: .now)
    )
    DashboardView(store: store)
        .environment(theme)
        .frame(width: 760, height: 640)
}
