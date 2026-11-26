import KodosiBridge
import SwiftUI

/// The key of the facts row that opens the agent's own settings, which `/config` also opens.
struct ChatSettingsKey: View {
    @Environment(\.theme) private var theme
    let chat: TerminalChat
    @Binding var open: Bool

    var body: some View {
        Button { open.toggle() } label: {
            Image(systemName: "gearshape")
                .font(scaled: 12, weight: .semibold)
                .foregroundStyle(open ? theme.colors.accentStrong : theme.colors.inkMuted)
                .padding(.horizontal, -3)
        }
        .buttonStyle(.kodosi(open ? .tinted : .secondary, size: .small))
        .animation(theme.motion.snappy, value: open)
        #if os(iOS)
            .sheet(isPresented: $open) {
                NavigationStack {
                    ChatSettings(chat: chat)
                        .navigationTitle(Text("Settings", bundle: .module))
                        .navigationBarTitleDisplayMode(.inline)
                        .toolbar {
                            ToolbarItem(placement: .confirmationAction) {
                                Button { open = false } label: { Text("Done", bundle: .module) }
                            }
                        }
                }
                .presentationDetents([.medium, .large])
            }
        #else
            .kodosiPopover(isPresented: $open, arrowEdge: .top, alignment: .leading) {
                ChatSettings(chat: chat).frame(width: 400)
            }
        #endif
            .help(Text("Settings", bundle: .module))
            .accessibilityLabel(Text("Settings", bundle: .module))
            .accessibilityIdentifier("chat.settings.key")
    }
}

/// The agent's own settings, as its settings view in the terminal lists them. The agent checks
/// and saves each change, and says why when it does not take one.
struct ChatSettings: View {
    @Environment(\.theme) private var theme
    let chat: TerminalChat
    @State private var finding = ""

    private var shown: [ChatSetting] {
        let words = finding.trimmingCharacters(in: .whitespaces)
        guard !words.isEmpty else { return chat.settings }
        return chat.settings.filter {
            $0.label.localizedCaseInsensitiveContains(words) || $0.description.localizedCaseInsensitiveContains(words)
        }
    }

    var body: some View {
        content
            .onAppear { chat.askSettings() }
            .animation(theme.motion.fade, value: chat.settings.isEmpty)
    }

    private var content: some View {
        #if os(iOS)
            List {
                rows
            }
            .listStyle(.plain)
            .searchable(text: $finding, prompt: Text("Find a setting", bundle: .module))
            .overlay { waiting }
        #else
            VStack(spacing: 8) {
                TextField(text: $finding) { Text("Find a setting", bundle: .module) }
                    .textFieldStyle(.plain)
                    .appTextStyle(.body)
                    .padding(.horizontal, 10).frame(height: 30)
                    .well(Radius.md)
                    .accessibilityIdentifier("chat.settings.find")
                if chat.settings.isEmpty {
                    waiting
                } else {
                    ScrollView {
                        LazyVStack(alignment: .leading, spacing: 0) { rows }
                    }
                    .frame(maxHeight: 420)
                    .fixedSize(horizontal: false, vertical: true)
                }
            }
            .padding(10)
        #endif
    }

    @ContentBuilder private var rows: some View {
        let shown = shown
        ForEach(shown) { setting in
            ChatSettingRow(setting: setting) { chat.set(setting, to: $0) }
        }
        if shown.isEmpty, !chat.settings.isEmpty {
            Text("No setting matches", bundle: .module)
                .appTextStyle(.footnote).foregroundStyle(theme.colors.inkFaint)
                .frame(maxWidth: .infinity).padding(.vertical, 16)
        }
    }

    @ContentBuilder private var waiting: some View {
        if chat.settings.isEmpty {
            ProgressView().controlSize(.small).frame(maxWidth: .infinity, minHeight: 120)
        }
    }
}

private struct ChatSettingRow: View {
    @Environment(\.theme) private var theme
    let setting: ChatSetting
    let set: (String) -> Void
    @State private var draft = ""

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack(alignment: .center, spacing: 10) {
                VStack(alignment: .leading, spacing: 2) {
                    HStack(spacing: 5) {
                        Text(verbatim: setting.label).appTextStyle(.body).foregroundStyle(theme.colors.ink)
                        if setting.locked {
                            Image(systemName: "lock.fill")
                                .font(scaled: 9, weight: .semibold).foregroundStyle(theme.colors.inkFaint)
                                .accessibilityLabel(Text("Locked", bundle: .module))
                        }
                    }
                    if !setting.description.isEmpty {
                        Text(verbatim: setting.description).appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted)
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                control.disabled(setting.locked)
            }
            if let refusal = setting.refusal {
                Text(verbatim: refusal).appTextStyle(.caption).foregroundStyle(theme.colors.caution)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
        .padding(.vertical, 8)
        #if os(macOS)
            .padding(.horizontal, 6)
        #endif
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier("chat.setting.\(setting.key)")
            .onAppear { draft = setting.value }
            .onChange(of: setting.value) { _, value in draft = value }
    }

    @ContentBuilder private var control: some View {
        switch setting.kind {
        case .toggle:
            Toggle(isOn: Binding(get: { setting.value == "true" }, set: { set($0 ? "true" : "false") })) {
                Text(verbatim: setting.label)
            }
            .labelsHidden()
            .toggleStyle(.switch)
            #if os(macOS)
                .controlSize(.small)
            #endif
        case .choice:
            Picker(selection: Binding(get: { setting.value }, set: set)) {
                ForEach(setting.options, id: \.self) { Text(verbatim: $0).tag($0) }
            } label: {
                Text(verbatim: setting.label)
            }
            .labelsHidden()
            .pickerStyle(.menu)
            .fixedSize()
        case .text, .number:
            TextField(text: $draft) { Text(verbatim: setting.label) }
                .labelsHidden()
                .multilineTextAlignment(.trailing)
                .appTextStyle(.body)
                .frame(width: 150)
            #if os(iOS)
                .keyboardType(setting.kind == .number ? .decimalPad : .default)
            #endif
                .onSubmit {
                    if draft != setting.value {
                        set(draft)
                    }
                }
        }
    }
}
