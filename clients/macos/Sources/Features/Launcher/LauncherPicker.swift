import KodosiDesign
import SwiftUI

struct PickerRow: Identifiable, Equatable {
    enum Kind: Equatable {
        case choice, action, note
    }

    enum Icon: Equatable {
        case none, folder
        case account(initial: String, key: String, signedIn: Bool)
        case device(String)
        case slot(String)
    }

    let id: String
    var kind = Kind.choice
    let title: String
    var subtitle: String?
    var detail: String?
    var icon = Icon.none
    var current = false
}

struct PickerKey<Glyph: View, Accessory: View>: View {
    @Environment(\.theme) private var theme
    let title: String
    var quiet = false
    var loading = false
    let open: Bool
    let identifier: String
    let label: String
    var help: String?
    @ViewBuilder let glyph: () -> Glyph
    @ViewBuilder let accessory: () -> Accessory
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack(spacing: 7) {
                glyph().squash(trigger: title)
                Text(verbatim: title)
                    .fontWeight(.medium)
                    .foregroundStyle(quiet ? theme.colors.inkMuted : theme.colors.ink)
                    .lineLimit(1).truncationMode(.middle)
                    .frame(maxWidth: 130, alignment: .leading)
                    .fixedSize(horizontal: true, vertical: false)
                    .contentTransition(.interpolate)
                    .shimmer(loading)
                accessory()
                Image(systemName: "chevron.down")
                    .font(.system(size: 8, weight: .bold))
                    .foregroundStyle(open ? theme.colors.accentStrong : theme.colors.inkFaint)
                    .rotationEffect(.degrees(open ? 180 : 0))
            }
            .padding(.horizontal, -3)
        }
        .buttonStyle(.kodosi(open ? .tinted : .secondary, size: .small))
        .animation(theme.motion.snappy, value: open)
        .animation(theme.motion.spring, value: title)
        .help(help ?? label)
        .accessibilityLabel(Text(verbatim: label))
        .accessibilityValue(Text(verbatim: title))
        .accessibilityAddTraits(open ? .isSelected : [])
        .accessibilityIdentifier(identifier)
    }
}

extension PickerKey where Accessory == EmptyView {
    init(
        title: String, quiet: Bool = false, loading: Bool = false, open: Bool, identifier: String, label: String,
        help: String? = nil, @ViewBuilder glyph: @escaping () -> Glyph, action: @escaping () -> Void,
    ) {
        self.init(
            title: title, quiet: quiet, loading: loading, open: open, identifier: identifier, label: label, help: help,
            glyph: glyph, accessory: { EmptyView() }, action: action,
        )
    }
}

struct PickerList: View {
    @Environment(\.theme) private var theme
    @Namespace private var pill
    let rows: [PickerRow]
    let identifier: String
    var width: CGFloat = 300
    let pick: (String) -> Void
    let close: () -> Void
    @State private var highlighted: String?
    @State private var filter = ""
    @State private var shown = 0
    @FocusState private var focused: Bool

    private static let visibleRows = 7

    private var rowHeight: CGFloat {
        rows.contains { $0.subtitle != nil } ? 44 : 32
    }

    private var matches: [PickerRow] {
        let query = filter.trimmingCharacters(in: .whitespaces)
        guard !query.isEmpty else { return rows }
        return rows.filter { row in
            row.kind != .note && [row.title, row.subtitle, row.detail].compactMap(\.self).contains {
                $0.localizedCaseInsensitiveContains(query)
            }
        }
    }

    var body: some View {
        let matches = matches
        VStack(alignment: .leading, spacing: 0) {
            if rows.count > Self.visibleRows || !filter.isEmpty {
                filterWell.padding(.horizontal, 8).padding(.top, 8)
            }
            ScrollViewReader { proxy in
                ScrollView(.vertical) {
                    VStack(spacing: 2) {
                        ForEach(Array(matches.enumerated()), id: \.element.id) { index, row in
                            item(row)
                                .opacity(index < shown ? 1 : 0)
                                .offset(y: index < shown ? 0 : -5)
                                .id(row.id)
                        }
                        if matches.isEmpty {
                            Text("Nothing matches")
                                .appTextStyle(.footnote).foregroundStyle(theme.colors.inkFaint)
                                .frame(maxWidth: .infinity, minHeight: rowHeight)
                        }
                    }
                    .padding(6)
                }
                .scrollIndicators(matches.count > Self.visibleRows ? .automatic : .never)
                .scrollDisabled(matches.count <= Self.visibleRows)
                .frame(height: CGFloat(min(max(matches.count, 1), Self.visibleRows)) * (rowHeight + 2) + 10)
                .onChange(of: highlighted) { _, id in
                    if let id {
                        proxy.scrollTo(id)
                    }
                }
            }
        }
        .frame(width: width)
        .focusable()
        .focused($focused)
        .focusEffectDisabled()
        .onKeyPress(.upArrow) { move(-1) }
        .onKeyPress(.downArrow) { move(1) }
        .onKeyPress(.return) {
            if let highlighted, matches.contains(where: { $0.id == highlighted }) {
                pick(highlighted)
            }
            return .handled
        }
        .onKeyPress(.delete) {
            guard !filter.isEmpty else { return .ignored }
            filter.removeLast()
            return .handled
        }
        .onKeyPress(phases: .down) { press in
            guard press.modifiers.isDisjoint(with: [.command, .control, .option]),
                  let character = press.characters.first, !character.isNewline, character.isLetter || character.isNumber
                  || character.isPunctuation || character.isSymbol || (character == " " && !filter.isEmpty)
            else { return .ignored }
            filter.append(press.characters)
            return .handled
        }
        .onChange(of: filter) { _, _ in
            highlighted = self.matches.first { $0.kind != .note }?.id
        }
        .onChange(of: focused) { _, focused in
            if !focused {
                close()
            }
        }
        .task(id: identifier) { await reveal() }
        .onAppear {
            focused = true
            highlighted = (rows.first(where: \.current) ?? rows.first { $0.kind != .note })?.id
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(identifier)
    }

    private var filterWell: some View {
        HStack(spacing: 7) {
            Image(systemName: "magnifyingglass").font(.system(size: 11, weight: .semibold)).foregroundStyle(theme.colors.inkFaint)
            if filter.isEmpty {
                Text("Type to find").appTextStyle(.footnote).foregroundStyle(theme.colors.inkFaint)
            } else {
                Text(verbatim: filter).appTextStyle(.footnote).foregroundStyle(theme.colors.ink)
                CursorBlock(width: 2, height: 14, blinks: false)
            }
            Spacer(minLength: 0)
        }
        .padding(.horizontal, 10).frame(height: 28)
        .well(Radius.sm)
    }

    private func item(_ row: PickerRow) -> some View {
        let lit = row.id == highlighted && row.kind != .note
        return HStack(spacing: 10) {
            PickerIcon(icon: row.icon)
            VStack(alignment: .leading, spacing: 1) {
                Text(verbatim: row.title)
                    .appTextStyle(.body).fontWeight(.medium)
                    .foregroundStyle(row.kind == .action ? theme.colors.accentStrong
                        : row.kind == .note ? theme.colors.inkFaint : theme.colors.ink)
                if let subtitle = row.subtitle {
                    Text(verbatim: subtitle)
                        .appTextStyle(row.icon == .folder ? .monoCaption : .footnote)
                        .foregroundStyle(theme.colors.inkMuted)
                        .truncationMode(.middle)
                }
            }
            .lineLimit(1)
            Spacer(minLength: 8)
            if let detail = row.detail {
                Text(verbatim: detail)
                    .appTextStyle(.monoCaption).foregroundStyle(theme.colors.inkFaint)
                    .lineLimit(1).truncationMode(.middle)
                    .frame(maxWidth: 130, alignment: .trailing)
            }
            if row.current {
                Image(systemName: "checkmark")
                    .font(.system(size: 11, weight: .bold))
                    .foregroundStyle(theme.colors.accentStrong)
                    .transition(AnyTransition.pop)
            }
        }
        .padding(.horizontal, 10)
        .frame(height: rowHeight)
        .background {
            if lit {
                SelectionPill(namespace: pill, radius: Radius.sm)
            }
        }
        .contentShape(Rectangle())
        .onHover { inside in
            if inside, row.kind != .note, highlighted != row.id {
                withAnimation(theme.motion.snappy) { highlighted = row.id }
            }
        }
        .onTapGesture {
            if row.kind != .note {
                pick(row.id)
            }
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(Text(verbatim: [row.title, row.subtitle, row.detail].compactMap(\.self).joined(separator: ", ")))
        .accessibilityAddTraits(row.kind == .note ? .isStaticText : row.current ? [.isButton, .isSelected] : .isButton)
        .accessibilityAction {
            if row.kind != .note {
                pick(row.id)
            }
        }
        .accessibilityIdentifier("\(identifier).\(AccessibilityIdentifier.token(row.id))")
    }

    private func move(_ step: Int) -> KeyPress.Result {
        let choices = matches.filter { $0.kind != .note }
        guard !choices.isEmpty else { return .handled }
        let index = choices.firstIndex { $0.id == highlighted } ?? (step > 0 ? -1 : 0)
        highlighted = choices[(index + step + choices.count) % choices.count].id
        return .handled
    }

    private func reveal() async {
        guard !theme.motion.reduced else {
            shown = .max
            return
        }
        shown = 0
        for index in 1 ... max(rows.count, 1) {
            withAnimation(theme.motion.fade) { shown = index }
            try? await Task.sleep(for: .milliseconds(18))
            guard !Task.isCancelled else { return }
        }
        shown = .max
    }
}

struct PickerIcon: View {
    let icon: PickerRow.Icon

    var body: some View {
        switch icon {
        case .none:
            EmptyView()
        case .folder:
            IconTile(symbol: "folder.fill", tint: TileTint.teal, size: 26)
        case let .account(initial, key, signedIn):
            AccountMark(initial: initial, key: key, signedIn: signedIn, size: 26)
        case let .device(label):
            IconTile(symbol: DeviceGlyph.symbol(for: label), tint: TileTint.graphite, size: 26)
        case let .slot(symbol):
            SlotMark(symbol: symbol, round: symbol != "folder", size: 26)
        }
    }
}

struct AccountMark: View {
    let initial: String
    let key: String
    let signedIn: Bool
    var size: CGFloat = 26

    var body: some View {
        Group {
            if signedIn {
                PersonAvatar(name: initial, key: key, size: size)
            } else {
                let tint = PersonTint.color(for: key)
                Text(verbatim: initial.uppercased())
                    .font(.system(size: size * 0.4, weight: .semibold, design: .rounded))
                    .foregroundStyle(tint)
                    .frame(width: size, height: size)
                    .overlay { Circle().strokeBorder(tint, style: StrokeStyle(lineWidth: 1.2, dash: [2.6, 2.2])) }
            }
        }
        .accessibilityHidden(true)
    }
}

struct SlotMark: View {
    @Environment(\.theme) private var theme
    let symbol: String
    var round = true
    var size: CGFloat = 26

    var body: some View {
        let stroke = StrokeStyle(lineWidth: 1.2, dash: [2.6, 2.2])
        Image(systemName: symbol)
            .font(.system(size: size * 0.4, weight: .bold))
            .foregroundStyle(theme.colors.accentStrong)
            .frame(width: size, height: size)
            .overlay {
                if round {
                    Circle().strokeBorder(theme.colors.accent.opacity(0.75), style: stroke)
                } else {
                    RoundedRectangle(cornerRadius: size * 0.3, style: .continuous)
                        .strokeBorder(theme.colors.accent.opacity(0.75), style: stroke)
                }
            }
            .accessibilityHidden(true)
    }
}

struct NewAccountForm: View {
    @Environment(\.theme) private var theme
    let agent: String
    let folder: String
    let error: String?
    let saving: Bool
    let add: (String) -> Void
    @State private var name = ""
    @FocusState private var focused: Bool

    private var slug: String {
        name.trimmingCharacters(in: .whitespaces).lowercased().split(separator: " ").joined(separator: "-")
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 10) {
                AccountMark(initial: slug.isEmpty ? "+" : String(slug.prefix(1)), key: "\(folder)-\(slug)", signedIn: false, size: 30)
                    .animation(theme.motion.snappy, value: slug.prefix(1))
                TextField("Name", text: $name, prompt: Text("work, personal…"))
                    .focused($focused)
                    .onSubmit(submit)
                    .shimmer(saving)
                    .accessibilityLabel(Text("Account name"))
                    .accessibilityIdentifier("launcher.account.name")
                Button("Add", action: submit)
                    .buttonStyle(.kodosi(.primary))
                    .disabled(slug.isEmpty || saving)
                    .accessibilityIdentifier("launcher.account.add")
            }
            Text(verbatim: slug.isEmpty ? String(localized: "\(agent) asks you to sign in the first time.") : "\(folder)-\(slug)")
                .appTextStyle(slug.isEmpty ? .footnote : .monoCaption)
                .foregroundStyle(theme.colors.inkFaint)
                .padding(.leading, 40)
                .contentTransition(.interpolate)
                .animation(theme.motion.snappy, value: slug)
            if let error {
                Text(verbatim: error).appTextStyle(.footnote).foregroundStyle(theme.colors.caution)
                    .fixedSize(horizontal: false, vertical: true)
                    .padding(.leading, 40)
                    .transition(.opacity)
            }
        }
        .padding(14)
        .frame(width: 340)
        .textFieldStyle(WellTextFieldStyle())
        .onAppear { focused = true }
    }

    private func submit() {
        guard !slug.isEmpty, !saving else { return }
        add(name)
    }
}
