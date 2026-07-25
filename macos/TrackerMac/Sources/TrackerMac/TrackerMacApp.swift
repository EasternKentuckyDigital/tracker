import SwiftUI

@main
@MainActor
struct TrackerMacApp: App {
    @State private var store: TrackerStore
    @State private var theme = TrackerTheme()
    @State private var securitySettings: TrackerSecuritySettings

    init() {
        let securitySettings = TrackerSecuritySettings()
        _securitySettings = State(initialValue: securitySettings)
        _store = State(
            initialValue: TrackerStore(securitySettings: securitySettings)
        )
    }

    var body: some Scene {
        Window("Tracker", id: "dashboard") {
            DashboardView(store: store)
                .environment(theme)
                .preferredColorScheme(theme.preferredColorScheme)
                .frame(minWidth: 620, minHeight: 560)
        }
        .defaultSize(width: 760, height: 660)
        .windowResizability(.contentMinSize)
        .windowToolbarStyle(.unifiedCompact)
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
            TrackerSettingsView(securitySettings: securitySettings)
                .environment(theme)
                .preferredColorScheme(theme.preferredColorScheme)
        }
    }
}
