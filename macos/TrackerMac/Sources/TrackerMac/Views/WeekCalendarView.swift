import SwiftUI

struct WeekCalendarView: View {
    @Environment(TrackerTheme.self) private var theme
    let store: TrackerStore

    private let hourLabelWidth: CGFloat = 46

    var body: some View {
        VStack(spacing: 0) {
            weekSummary
            dayHeader
            Divider()

            ScrollView(.vertical) {
                timeline
            }
            .defaultScrollAnchor(.center)
        }
        .background(theme.primaryBackground)
    }

    private var weekSummary: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(alignment: .firstTextBaseline) {
                Text(weekTitle)
                    .font(.headline)
                Spacer()
                Text(Duration.compact(weekTotal))
                    .font(.title3.bold().monospacedDigit())
                Text("tracked")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }

            if !projectTotals.isEmpty {
                ScrollView(.horizontal) {
                    LazyHStack(spacing: 7) {
                        ForEach(projectTotals.prefix(6)) { total in
                            HStack(spacing: 5) {
                                Circle()
                                    .fill(EntryColor.color(
                                        for: total.name,
                                        fallback: theme.accent.color
                                    ))
                                    .frame(width: 6, height: 6)
                                Text(total.name)
                                    .foregroundStyle(.secondary)
                                Text(Duration.compact(total.seconds))
                                    .fontWeight(.semibold)
                                    .monospacedDigit()
                            }
                            .font(.caption)
                            .padding(.horizontal, 8)
                            .padding(.vertical, 5)
                            .background(
                                theme.secondaryBackground,
                                in: Capsule()
                            )
                            .overlay {
                                Capsule().stroke(theme.subtleBorder)
                            }
                        }
                    }
                }
                .scrollIndicators(.hidden)
            } else {
                Text("No tracked time in this week")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
        .padding(.horizontal, 16)
        .padding(.vertical, theme.density == .compact ? 8 : 11)
    }

    private var dayHeader: some View {
        HStack(spacing: 0) {
            Color.clear.frame(width: hourLabelWidth)
            ForEach(visibleDays, id: \.self) { day in
                VStack(spacing: 3) {
                    Text(day.formatted(.dateTime.weekday(.abbreviated)))
                        .font(.caption.weight(.semibold))
                        .foregroundStyle(Calendar.current.isDateInToday(day) ? theme.accent.color : .secondary)
                    Text(day.formatted(.dateTime.day()))
                        .font(.system(size: 17, weight: Calendar.current.isDateInToday(day) ? .bold : .medium))
                        .frame(width: 28, height: 25)
                        .background {
                            if Calendar.current.isDateInToday(day) {
                                Circle().fill(theme.accent.color.opacity(0.14))
                            }
                        }
                    Text(Duration.compact(total(for: day)))
                        .font(.caption2)
                        .foregroundStyle(.tertiary)
                }
                .frame(maxWidth: .infinity)
                .padding(.vertical, 7)
                .accessibilityElement(children: .combine)
            }
        }
    }

    private var timeline: some View {
        let hours = theme.calendarStartHour..<theme.calendarEndHour
        let height = CGFloat(hours.count) * theme.density.hourHeight

        return ZStack(alignment: .topLeading) {
            HStack(spacing: 0) {
                Color.clear.frame(width: hourLabelWidth)

                ForEach(visibleDays, id: \.self) { day in
                    DayTimelineColumn(
                        day: day,
                        entries: allEntries,
                        startHour: theme.calendarStartHour,
                        endHour: theme.calendarEndHour
                    )
                    .frame(maxWidth: .infinity)
                    .overlay(alignment: .leading) {
                        Divider().opacity(0.55)
                    }
                }
            }
            .frame(height: height)

            VStack(spacing: 0) {
                ForEach(Array(hours), id: \.self) { hour in
                    HStack(spacing: 5) {
                        Text(hourLabel(hour))
                            .font(.caption2.monospacedDigit())
                            .foregroundStyle(.tertiary)
                            .frame(width: hourLabelWidth - 6, alignment: .trailing)
                        Divider()
                    }
                    .frame(height: theme.density.hourHeight, alignment: .top)
                }
            }
            .allowsHitTesting(false)
        }
    }

    private var visibleDays: [Date] {
        let days = (0..<7).compactMap {
            Calendar.current.date(byAdding: .day, value: $0, to: store.selectedWeekStart)
        }
        guard !theme.showWeekends else { return days }
        return days.filter {
            let weekday = Calendar.current.component(.weekday, from: $0)
            return weekday != 1 && weekday != 7
        }
    }

    private var allEntries: [TrackerEntry] {
        let candidates: [TrackerEntry]
        guard let active = store.snapshot.activeEntry,
              !store.snapshot.entries.contains(where: { $0.id == active.id })
        else {
            candidates = store.snapshot.entries
            return entriesInSelectedWeek(candidates)
        }
        candidates = store.snapshot.entries + [active]
        return entriesInSelectedWeek(candidates)
    }

    private var weekTitle: String {
        guard let last = Calendar.current.date(byAdding: .day, value: 6, to: store.selectedWeekStart)
        else { return "This week" }
        return "\(store.selectedWeekStart.formatted(.dateTime.month(.abbreviated).day())) – \(last.formatted(.dateTime.month(.abbreviated).day().year()))"
    }

    private var weekTotal: TimeInterval {
        allEntries.reduce(0) { $0 + $1.elapsed() }
    }

    private var projectTotals: [ProjectTotal] {
        let totals = Dictionary(grouping: allEntries) { $0.project ?? "No project" }
            .mapValues { $0.reduce(0) { $0 + $1.elapsed() } }
        return totals
            .map { ProjectTotal(name: $0.key, seconds: $0.value) }
            .sorted { $0.seconds > $1.seconds }
    }

    private func total(for day: Date) -> TimeInterval {
        let calendar = Calendar.current
        let start = calendar.startOfDay(for: day)
        guard let end = calendar.date(byAdding: .day, value: 1, to: start) else { return 0 }
        return allEntries.reduce(0) { result, entry in
            let entryEnd = entry.stoppedAt ?? .now
            let overlapStart = max(entry.startedAt, start)
            let overlapEnd = min(entryEnd, end)
            return result + max(0, overlapEnd.timeIntervalSince(overlapStart))
        }
    }

    private func entriesInSelectedWeek(_ entries: [TrackerEntry]) -> [TrackerEntry] {
        guard let weekEnd = Calendar.current.date(
            byAdding: .day,
            value: 7,
            to: store.selectedWeekStart
        ) else { return entries }
        return entries.filter {
            $0.startedAt < weekEnd && ($0.stoppedAt ?? .now) > store.selectedWeekStart
        }
    }

    private func hourLabel(_ hour: Int) -> String {
        let date = Calendar.current.date(from: DateComponents(hour: hour)) ?? .now
        return date.formatted(.dateTime.hour())
    }
}

private struct DayTimelineColumn: View {
    @Environment(TrackerTheme.self) private var theme
    let day: Date
    let entries: [TrackerEntry]
    let startHour: Int
    let endHour: Int

    var body: some View {
        GeometryReader { geometry in
            TimelineView(.periodic(from: .now, by: 60)) { context in
                ZStack(alignment: .topLeading) {
                    if Calendar.current.isDateInToday(day) {
                        theme.accent.color.opacity(0.025)
                    }

                    ForEach(placements(at: context.date)) { placement in
                        CalendarEntryBlock(entry: placement.entry)
                            .frame(
                                width: max(28, geometry.size.width - 6),
                                height: placement.height
                            )
                            .offset(x: 3, y: placement.y)
                    }

                    if Calendar.current.isDateInToday(day),
                       let y = currentTimeY(at: context.date) {
                        HStack(spacing: 0) {
                            Circle()
                                .fill(.red)
                                .frame(width: 6, height: 6)
                            Rectangle()
                                .fill(.red)
                                .frame(height: 1)
                        }
                        .offset(y: y)
                    }
                }
                .clipped()
            }
        }
    }

    private func placements(at now: Date) -> [EntryPlacement] {
        let calendar = Calendar.current
        let dayStart = calendar.startOfDay(for: day)
        guard let visibleStart = calendar.date(byAdding: .hour, value: startHour, to: dayStart),
              let visibleEnd = calendar.date(byAdding: .hour, value: endHour, to: dayStart)
        else { return [] }

        return entries.compactMap { entry in
            let entryEnd = entry.stoppedAt ?? now
            let clippedStart = max(entry.startedAt, visibleStart)
            let clippedEnd = min(entryEnd, visibleEnd)
            guard clippedEnd > clippedStart else { return nil }

            let y = CGFloat(clippedStart.timeIntervalSince(visibleStart) / 3_600)
                * theme.density.hourHeight
            let durationHeight = CGFloat(clippedEnd.timeIntervalSince(clippedStart) / 3_600)
                * theme.density.hourHeight
            return EntryPlacement(
                entry: entry,
                y: y,
                height: max(22, durationHeight - 2)
            )
        }
    }

    private func currentTimeY(at now: Date) -> CGFloat? {
        let calendar = Calendar.current
        let dayStart = calendar.startOfDay(for: day)
        guard let visibleStart = calendar.date(byAdding: .hour, value: startHour, to: dayStart),
              let visibleEnd = calendar.date(byAdding: .hour, value: endHour, to: dayStart),
              now >= visibleStart,
              now <= visibleEnd
        else { return nil }
        return CGFloat(now.timeIntervalSince(visibleStart) / 3_600)
            * theme.density.hourHeight
    }
}

private struct EntryPlacement: Identifiable {
    let entry: TrackerEntry
    let y: CGFloat
    let height: CGFloat
    var id: String { entry.id }
}

private struct ProjectTotal: Identifiable {
    let name: String
    let seconds: TimeInterval
    var id: String { name }
}

private struct CalendarEntryBlock: View {
    @Environment(TrackerTheme.self) private var theme
    let entry: TrackerEntry

    var body: some View {
        VStack(alignment: .leading, spacing: 1) {
            Text(entry.taskName)
                .font(.system(size: 10.5 * theme.fontScale, weight: .semibold))
                .lineLimit(2)
            if theme.showProjectsOnBlocks, let project = entry.project {
                Text(project)
                    .font(.system(size: 9 * theme.fontScale))
                    .lineLimit(1)
                    .opacity(0.75)
            }
        }
        .foregroundStyle(color.accessibleForeground)
        .padding(.horizontal, 5)
        .padding(.vertical, 3)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .background(color.opacity(0.86), in: RoundedRectangle(cornerRadius: 5))
        .overlay(alignment: .leading) {
            Rectangle()
                .fill(color)
                .frame(width: 3)
        }
        .help("\(entry.taskName) • \(Duration.compact(entry.elapsed()))")
        .accessibilityLabel("\(entry.taskName), \(Duration.compact(entry.elapsed()))")
    }

    private var color: Color {
        let key = entry.tags.first ?? entry.project ?? entry.taskName
        return EntryColor.color(for: key, fallback: theme.accent.color)
    }
}

enum EntryColor {
    private static let palette: [Color] = [
        .indigo, .blue, .teal, .green, .orange, .pink, .purple
    ]

    static func color(for key: String, fallback: Color) -> Color {
        guard !key.isEmpty else { return fallback }
        let stableValue = key.unicodeScalars.reduce(UInt(0)) {
            ($0 &* 31) &+ UInt($1.value)
        }
        return palette[Int(stableValue % UInt(palette.count))]
    }
}

private extension Color {
    var accessibleForeground: Color {
        .white
    }
}

#Preview("Week calendar") {
    let theme = TrackerTheme()
    let store = TrackerStore(
        initialSnapshot: .preview,
        weekStart: theme.weekStart(containing: .now)
    )
    WeekCalendarView(store: store)
        .environment(theme)
        .frame(width: 680, height: 620)
}
