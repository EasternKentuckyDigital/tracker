import SwiftUI

struct TrackerSettingsView: View {
    @Environment(TrackerTheme.self) private var theme
    let securitySettings: TrackerSecuritySettings

    var body: some View {
        TabView {
            AppearanceSettings(theme: theme)
                .tabItem { Label("Appearance", systemImage: "paintpalette") }

            CalendarSettings(theme: theme)
                .tabItem { Label("Calendar", systemImage: "calendar") }

            SyncSettings(theme: theme, securitySettings: securitySettings)
                .tabItem { Label("Sync", systemImage: "arrow.triangle.2.circlepath") }
        }
        .scenePadding()
        .frame(width: 500, height: 410)
    }
}

private struct AppearanceSettings: View {
    @Bindable var theme: TrackerTheme

    var body: some View {
        Form {
            Section("Accent") {
                HStack(spacing: 12) {
                    ForEach(TrackerAccent.allCases) { accent in
                        Button {
                            theme.accent = accent
                        } label: {
                            Circle()
                                .fill(accent.color)
                                .frame(width: 24, height: 24)
                                .overlay {
                                    if theme.accent == accent {
                                        Image(systemName: "checkmark")
                                            .font(.caption.bold())
                                            .foregroundStyle(.white)
                                    }
                                }
                        }
                        .buttonStyle(.plain)
                        .accessibilityLabel("\(accent.title) accent")
                    }
                }
            }

            Picker("Appearance", selection: $theme.appearance) {
                ForEach(TrackerAppearance.allCases) { appearance in
                    Text(appearance.title).tag(appearance)
                }
            }
            .pickerStyle(.segmented)

            Picker("Layout density", selection: $theme.density) {
                ForEach(TrackerDensity.allCases) { density in
                    Text(density.title).tag(density)
                }
            }
            .pickerStyle(.segmented)

            LabeledContent("Text size") {
                HStack {
                    Slider(value: $theme.fontScale, in: 0.85...1.25, step: 0.05)
                    Text("\(theme.fontScale, format: .number.precision(.fractionLength(2)))×")
                        .monospacedDigit()
                        .frame(width: 42)
                }
            }
        }
        .formStyle(.grouped)
    }
}

private struct CalendarSettings: View {
    @Bindable var theme: TrackerTheme

    var body: some View {
        Form {
            Toggle("Start weeks on Monday", isOn: $theme.weekStartsMonday)
            Toggle("Show weekends", isOn: $theme.showWeekends)
            Toggle("Show project names on time blocks", isOn: $theme.showProjectsOnBlocks)

            LabeledContent("Visible hours") {
                HStack {
                    Picker("Start", selection: $theme.calendarStartHour) {
                        ForEach(0..<18, id: \.self) { hour in
                            Text(hourName(hour)).tag(hour)
                        }
                    }
                    .labelsHidden()

                    Text("to")
                        .foregroundStyle(.secondary)

                    Picker("End", selection: $theme.calendarEndHour) {
                        ForEach((theme.calendarStartHour + 1)..<25, id: \.self) { hour in
                            Text(hourName(hour)).tag(hour)
                        }
                    }
                    .labelsHidden()
                }
            }
        }
        .formStyle(.grouped)
    }

    private func hourName(_ hour: Int) -> String {
        let normalizedHour = hour == 24 ? 0 : hour
        let date = Calendar.current.date(from: DateComponents(hour: normalizedHour)) ?? .now
        return date.formatted(.dateTime.hour())
    }
}

private struct SyncSettings: View {
    @Bindable var theme: TrackerTheme
    @Bindable var securitySettings: TrackerSecuritySettings
    @State private var tokenDraft = ""
    @State private var tokenFeedback: String?
    @State private var tokenFeedbackIsError = false

    var body: some View {
        Form {
            Section("Behavior") {
                Toggle("Sync after starting or stopping a timer", isOn: $theme.syncAfterChanges)
                Toggle("Sync periodically while Tracker is open", isOn: $theme.periodicSyncEnabled)

                LabeledContent("Sync interval") {
                    Picker("Sync interval", selection: $theme.syncIntervalMinutes) {
                        Text("5 minutes").tag(5)
                        Text("15 minutes").tag(15)
                        Text("30 minutes").tag(30)
                        Text("1 hour").tag(60)
                    }
                    .labelsHidden()
                    .disabled(!theme.periodicSyncEnabled)
                }
            }

            Section("Peer") {
                TextField(
                    "Automatic Tailscale discovery",
                    text: $securitySettings.manualPeerURL
                )
                .textContentType(.URL)
                .disableAutocorrection(true)
                .onChange(of: securitySettings.manualPeerURL) {
                    securitySettings.manualPeerURL =
                        securitySettings.manualPeerURL.limitedToUTF8Bytes(2_048)
                }
                .accessibilityLabel("Manual Tracker peer URL")

                Text(
                    securitySettings.manualPeerURL
                        .trimmingCharacters(in: .whitespacesAndNewlines)
                        .isEmpty
                        ? "Leave blank to discover Tracker servers with Tailscale. Sandboxed builds should use an origin such as http://100.64.0.2:7789."
                        : "Manual peer enabled. Tracker accepts only an http or https origin with no credentials, query, or path."
                )
                .font(.caption)
                .foregroundStyle(.secondary)
            }

            Section("Application token") {
                LabeledContent(
                    securitySettings.hasSyncToken ? "Token stored" : "No token stored"
                ) {
                    Image(
                        systemName: securitySettings.hasSyncToken
                            ? "checkmark.shield.fill"
                            : "shield.slash"
                    )
                    .foregroundStyle(
                        securitySettings.hasSyncToken ? .green : .secondary
                    )
                }

                SecureField(
                    securitySettings.hasSyncToken
                        ? "Enter a replacement token"
                        : "At least 32 bytes",
                    text: $tokenDraft
                )
                .textContentType(.password)

                HStack {
                    Button(securitySettings.hasSyncToken ? "Replace Token" : "Save Token") {
                        saveToken()
                    }
                    .disabled(tokenDraft.isEmpty)

                    if securitySettings.hasSyncToken {
                        Button("Remove Token", role: .destructive) {
                            removeToken()
                        }
                    }
                }

                if let tokenFeedback {
                    Text(tokenFeedback)
                        .font(.caption)
                        .foregroundStyle(tokenFeedbackIsError ? .red : .secondary)
                } else {
                    Text("The token is stored in this Mac’s Keychain and is passed only to the signed Tracker helper.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
        }
        .formStyle(.grouped)
    }

    private func saveToken() {
        do {
            try securitySettings.saveToken(tokenDraft)
            tokenDraft = ""
            tokenFeedback = "Token saved securely."
            tokenFeedbackIsError = false
        } catch {
            tokenFeedback = error.localizedDescription
            tokenFeedbackIsError = true
        }
    }

    private func removeToken() {
        do {
            try securitySettings.saveToken("")
            tokenDraft = ""
            tokenFeedback = "Token removed from Keychain."
            tokenFeedbackIsError = false
        } catch {
            tokenFeedback = error.localizedDescription
            tokenFeedbackIsError = true
        }
    }
}

#Preview("Settings") {
    TrackerSettingsView(securitySettings: TrackerSecuritySettings())
        .environment(TrackerTheme())
}
