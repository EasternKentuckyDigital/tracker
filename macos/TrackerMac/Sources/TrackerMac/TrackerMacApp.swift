import SwiftUI

@main
@MainActor
struct TrackerMacApp: App {
    @State private var store = TrackerStore()
    @State private var theme = TrackerTheme()

    var body: some Scene {
        WindowGroup("Tracker", id: "dashboard") {
            DashboardView(store: store)
                .environment(theme)
                .preferredColorScheme(theme.preferredColorScheme)
                .frame(minWidth: 840, minHeight: 520)
        }
        .defaultSize(width: 960, height: 680)
        .windowResizability(.contentMinSize)
        .commands {
            CommandMenu("Timer") {
                Button("Stop Timer") {
                    Task { await store.stop(syncAfterChange: theme.syncAfterChanges) }
                }
                .keyboardShortcut(".", modifiers: [.command])
                .disabled(store.snapshot.activeEntry == nil)

                Divider()

                Button("Sync Now") {
                    Task { await store.sync() }
                }
                .keyboardShortcut("r", modifiers: [.command, .shift])
                .disabled(store.isWorking)
            }
        }

        MenuBarExtra {
            MenuBarTrackerView(store: store)
                .environment(theme)
                .preferredColorScheme(theme.preferredColorScheme)
        } label: {
            Image(systemName: store.snapshot.activeEntry == nil ? "clock" : "timer")
                .accessibilityLabel(
                    store.snapshot.activeEntry == nil
                        ? "Tracker"
                        : "Tracker, timer running"
                )
        }
        .menuBarExtraStyle(.window)

        Settings {
            TrackerSettingsView()
                .environment(theme)
                .preferredColorScheme(theme.preferredColorScheme)
        }
    }
}
