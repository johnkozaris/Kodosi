import KodosiDesign
import SwiftUI

struct KeyboardSettingsView: View {
    private struct Recording: Equatable {
        let action: AppShortcut
        let old: KeyChord?
    }

    private static let groups: [[AppShortcut]] = [
        [.newTerminal, .quickTerminal, .resume, .close, .minimize, .zoom, .details],
        [.previousTerminal, .nextTerminal, .leftTerminal, .rightTerminal, .upTerminal, .downTerminal, .numberedTerminal],
        [.goTo, .sidebar, .dosi, .stopDosi, .settings, .fullScreen],
    ]

    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    @State private var recording: Recording?
    @State private var note: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 22) {
            HStack(spacing: 12) {
                if let note {
                    Text(note).appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted)
                }
                Spacer()
                Button("Reset All") {
                    deps.settings.shortcuts = ShortcutTable()
                    note = nil
                }
                .buttonStyle(.kodosi(.secondary, size: .small))
                .disabled(!deps.settings.shortcuts.isChanged)
                .accessibilityIdentifier("settings.keyboard.reset")
            }
            ForEach(Self.groups, id: \.self) { group in
                ListGroup {
                    ForEach(group) { action in
                        row(action)
                    }
                }
            }
        }
        .onDisappear { stopRecording() }
    }

    private func row(_ action: AppShortcut) -> some View {
        HStack(spacing: 6) {
            Text(action.title).appTextStyle(.body).foregroundStyle(theme.colors.ink).lineLimit(1)
            Spacer(minLength: 12)
            ForEach(deps.settings.shortcuts.keys(for: action), id: \.self) { chord in
                KeyCap(
                    label: action.label(of: chord), recording: recording == Recording(action: action, old: chord),
                    identifier: "settings.keyboard.\(action.rawValue).\(chord.label)",
                ) {
                    startRecording(Recording(action: action, old: chord))
                } remove: {
                    deps.settings.shortcuts.remove(chord, from: action)
                }
            }
            if recording == Recording(action: action, old: nil) {
                KeyCap(label: "", recording: true, identifier: "settings.keyboard.\(action.rawValue).new") {
                    stopRecording()
                } remove: {}
            } else {
                Button { startRecording(Recording(action: action, old: nil)) } label: {
                    Image(systemName: "plus").font(.system(size: 11, weight: .semibold)).frame(width: 26, height: 26)
                }
                .buttonStyle(.kodosi(.ghost, size: .small))
                .tip("Add a key")
                .accessibilityLabel(Text("Add a key for \(action.title)"))
                .accessibilityIdentifier("settings.keyboard.\(action.rawValue).add")
            }
        }
        .padding(.horizontal, 14).frame(minHeight: 44)
    }

    private func startRecording(_ next: Recording) {
        note = nil
        recording = next
        deps.shortcutRecorder = { event in
            record(event, for: next)
            return true
        }
    }

    private func stopRecording() {
        recording = nil
        deps.shortcutRecorder = nil
    }

    private func record(_ event: NSEvent, for recording: Recording) {
        guard let chord = KeyChord(event: event), chord.key != "⎋" || !chord.modifiers.isEmpty else {
            stopRecording()
            return
        }
        if chord == KeyChord("⌫", []), let old = recording.old {
            deps.settings.shortcuts.remove(old, from: recording.action)
            stopRecording()
            return
        }
        guard chord.canBeShortcut else {
            note = String(localized: "Use ⌘, ⌃ or ⌥ with a key.")
            return
        }
        guard recording.action != .numberedTerminal || Int(chord.key) != nil else {
            note = String(localized: "Use a digit.")
            return
        }
        let holder = deps.settings.shortcuts.bind(chord, to: recording.action, replacing: recording.old)
        note = holder.map { String(localized: "\(chord.label) was \($0.title).") }
        stopRecording()
    }
}

private struct KeyCap: View {
    @Environment(\.theme) private var theme
    let label: String
    let recording: Bool
    let identifier: String
    let change: () -> Void
    let remove: () -> Void
    @State private var hovered = false

    var body: some View {
        Button(action: change) {
            Text(recording ? String(localized: "Type a key") : label)
                .appTextStyle(.caption).lineLimit(1).padding(.horizontal, 2)
        }
        .buttonStyle(.kodosi(recording ? .tinted : .secondary, size: .small))
        .overlay(alignment: .topTrailing) {
            if hovered, !recording {
                Button(action: remove) {
                    Image(systemName: "xmark").font(.system(size: 7, weight: .bold)).foregroundStyle(theme.colors.inkMuted)
                        .frame(width: 14, height: 14)
                        .background { BezelBackground(shape: Circle(), fill: theme.buttonFace(), bezel: .quiet) }
                }
                .buttonStyle(PressScaleStyle())
                .offset(x: 5, y: -5)
                .accessibilityLabel(Text("Remove \(label)"))
                .accessibilityIdentifier("\(identifier).remove")
            }
        }
        .onHover { hovered = $0 }
        .accessibilityIdentifier(identifier)
    }
}
