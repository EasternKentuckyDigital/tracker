import AppKit
import Observation
import SwiftUI

enum TrackerAccent: String, CaseIterable, Hashable, Identifiable {
    case indigo, blue, teal, green, amber, orange, pink, violet

    var id: String { rawValue }
    var title: String { rawValue.capitalized }

    var color: Color {
        switch self {
        case .indigo: .indigo
        case .blue: .blue
        case .teal: .teal
        case .green: .green
        case .amber: Color(red: 0.94, green: 0.66, blue: 0.12)
        case .orange: .orange
        case .pink: .pink
        case .violet: .purple
        }
    }
}

enum TrackerAppearance: String, CaseIterable, Hashable, Identifiable {
    case system, light, dark

    var id: String { rawValue }
    var title: String { rawValue.capitalized }
}

enum TrackerDensity: String, CaseIterable, Hashable, Identifiable {
    case compact, comfortable

    var id: String { rawValue }
    var title: String { rawValue.capitalized }
    var spacing: CGFloat { self == .compact ? 8 : 12 }
    var hourHeight: CGFloat { self == .compact ? 48 : 64 }
}

@MainActor
@Observable
final class TrackerTheme {
    @ObservationIgnored
    private let defaults: UserDefaults

    var accent: TrackerAccent {
        didSet { defaults.set(accent.rawValue, forKey: "appearance.accent") }
    }
    var appearance: TrackerAppearance {
        didSet { defaults.set(appearance.rawValue, forKey: "appearance.mode") }
    }
    var density: TrackerDensity {
        didSet { defaults.set(density.rawValue, forKey: "appearance.density") }
    }
    var fontScale: Double {
        didSet { defaults.set(fontScale, forKey: "appearance.fontScale") }
    }
    var weekStartsMonday: Bool {
        didSet { defaults.set(weekStartsMonday, forKey: "calendar.mondayFirst") }
    }
    var showWeekends: Bool {
        didSet { defaults.set(showWeekends, forKey: "calendar.showWeekends") }
    }
    var calendarStartHour: Int {
        didSet {
            if calendarEndHour <= calendarStartHour {
                calendarEndHour = min(24, calendarStartHour + 1)
            }
            defaults.set(calendarStartHour, forKey: "calendar.startHour")
        }
    }
    var calendarEndHour: Int {
        didSet {
            if calendarEndHour <= calendarStartHour {
                calendarStartHour = max(0, calendarEndHour - 1)
            }
            defaults.set(calendarEndHour, forKey: "calendar.endHour")
        }
    }
    var showProjectsOnBlocks: Bool {
        didSet { defaults.set(showProjectsOnBlocks, forKey: "calendar.showProjects") }
    }
    var syncAfterChanges: Bool {
        didSet { defaults.set(syncAfterChanges, forKey: "sync.afterChanges") }
    }
    var periodicSyncEnabled: Bool {
        didSet { defaults.set(periodicSyncEnabled, forKey: "sync.periodicEnabled") }
    }
    var syncIntervalMinutes: Int {
        didSet { defaults.set(syncIntervalMinutes, forKey: "sync.intervalMinutes") }
    }

    init(defaults: UserDefaults = .standard) {
        self.defaults = defaults
        accent = TrackerAccent(
            rawValue: defaults.string(forKey: "appearance.accent") ?? ""
        ) ?? .indigo
        appearance = TrackerAppearance(
            rawValue: defaults.string(forKey: "appearance.mode") ?? ""
        ) ?? .system
        density = TrackerDensity(
            rawValue: defaults.string(forKey: "appearance.density") ?? ""
        ) ?? .comfortable
        let savedFontScale = defaults.object(forKey: "appearance.fontScale") as? Double ?? 1
        fontScale = min(1.25, max(0.85, savedFontScale.isFinite ? savedFontScale : 1))
        weekStartsMonday = defaults.object(forKey: "calendar.mondayFirst") as? Bool ?? true
        showWeekends = defaults.object(forKey: "calendar.showWeekends") as? Bool ?? true
        let savedStart = defaults.object(forKey: "calendar.startHour") as? Int ?? 7
        let savedEnd = defaults.object(forKey: "calendar.endHour") as? Int ?? 22
        let clampedStart = min(23, max(0, savedStart))
        calendarStartHour = clampedStart
        calendarEndHour = min(24, max(clampedStart + 1, savedEnd))
        showProjectsOnBlocks = defaults.object(forKey: "calendar.showProjects") as? Bool ?? true
        syncAfterChanges = defaults.object(forKey: "sync.afterChanges") as? Bool ?? true
        periodicSyncEnabled = defaults.object(forKey: "sync.periodicEnabled") as? Bool ?? true
        let savedSyncInterval =
            defaults.object(forKey: "sync.intervalMinutes") as? Int ?? 15
        syncIntervalMinutes = [5, 15, 30, 60].contains(savedSyncInterval)
            ? savedSyncInterval
            : 15
    }

    var preferredColorScheme: ColorScheme? {
        switch appearance {
        case .system: nil
        case .light: .light
        case .dark: .dark
        }
    }

    var primaryBackground: Color {
        Color(nsColor: .windowBackgroundColor)
    }

    var secondaryBackground: Color {
        Color(nsColor: .controlBackgroundColor)
    }

    var subtleBorder: Color {
        Color.primary.opacity(0.09)
    }

    func weekStart(containing date: Date) -> Date {
        var calendar = Calendar.current
        calendar.firstWeekday = weekStartsMonday ? 2 : 1
        let components = calendar.dateComponents([.yearForWeekOfYear, .weekOfYear], from: date)
        return calendar.date(from: components) ?? calendar.startOfDay(for: date)
    }
}
