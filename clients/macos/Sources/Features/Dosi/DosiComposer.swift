import KodosiBridge
import KodosiDesign
import SwiftUI

struct DosiComposer: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    var focused: FocusState<Bool>.Binding

    private var typed: Bool {
        !deps.dosi.draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    private var stops: Bool {
        deps.dosi.busy && !typed
    }

    var body: some View {
        @Bindable var dosi = deps.dosi
        let shape = RoundedRectangle(cornerRadius: 20, style: .continuous)
        VStack(alignment: .leading, spacing: 6) {
            ForEach(dosi.queued) { queued in
                HStack(spacing: 6) {
                    Image(systemName: "clock").font(.system(size: 9, weight: .semibold)).foregroundStyle(theme.colors.inkFaint)
                    Text(queued.text).appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted).lineLimit(1)
                    Spacer(minLength: 2)
                    IconButton(title: "Do not send", symbol: "xmark", identifier: "dosi.queued.remove", size: 20) {
                        dosi.unqueue(queued)
                    }
                }
                .padding(.leading, 10).padding(.trailing, 2).frame(height: 26)
                .background(theme.colors.ink.opacity(0.05), in: Capsule())
                .transition(AnyTransition.rise)
            }
            HStack(alignment: .bottom, spacing: 8) {
                DosiFace(mood: dosi.mood(listening: focused.wrappedValue), size: 24)
                    .fold(FoldPart.face)
                TextField("Ask Dosi", text: $dosi.draft, axis: .vertical)
                    .lineLimit(1 ... 8).textFieldStyle(.plain).appTextStyle(.callout).focused(focused)
                    .padding(.vertical, 4)
                    .frame(minHeight: 24)
                    .accessibilityIdentifier("dosi.input")
                    .disabled(dosi.state == .unavailable)
                    .onKeyPress(.return, phases: .down) { press in
                        guard press.modifiers.isDisjoint(with: [.option, .shift]) else { return .ignored }
                        deps.sendToDosi()
                        return .handled
                    }
                    .onKeyPress(.escape) {
                        deps.workbench.closeDosi()
                        return .handled
                    }
                    .arrive(delay: 0.35, rise: 0)
                if !dosi.items.isEmpty {
                    DosiNewChatButton().transition(.scale(scale: 0.6).combined(with: .opacity))
                }
                action.frame(width: 28, height: 28)
            }
            .padding(.leading, 8).padding(.trailing, 6).padding(.vertical, 6)
            .background { WellBackground(shape: shape).fold(FoldPart.box) }
            .overlay {
                shape.strokeBorder(theme.colors.accent.opacity(focused.wrappedValue ? 0.4 : 0), lineWidth: 1)
            }
            .contentShape(shape)
            .onTapGesture { focused.wrappedValue = true }
        }
        .padding(.horizontal, 12).padding(.bottom, 12).padding(.top, 8)
        .animation(theme.motion.snappy, value: typed)
        .animation(theme.motion.snappy, value: stops)
        .animation(theme.motion.snappy, value: dosi.items.isEmpty)
        .animation(theme.motion.spring, value: dosi.queued.map(\.id))
        .animation(theme.motion.hover, value: focused.wrappedValue)
    }

    @ViewBuilder
    private var action: some View {
        if stops {
            Button { deps.dosi.stop() } label: {
                RoundedRectangle(cornerRadius: 2, style: .continuous).fill(theme.colors.ink).frame(width: 9, height: 9)
                    .frame(width: 28, height: 28)
                    .background { BezelBackground(shape: Circle(), fill: theme.colors.lifted, bezel: .quiet) }
            }
            .buttonStyle(PressScaleStyle(scale: 0.88))
            .help("Stop (⌘.)").accessibilityLabel(Text("Stop")).accessibilityIdentifier("dosi.stop")
            .transition(AnyTransition.pop)
        } else if typed {
            Button { deps.sendToDosi() } label: {
                Image(systemName: "arrow.up").font(.system(size: 13, weight: .bold)).foregroundStyle(theme.colors.onAccent)
                    .frame(width: 28, height: 28)
                    .background { BezelBackground(shape: Circle(), fill: theme.colors.accent) }
            }
            .buttonStyle(PressScaleStyle(scale: 0.88))
            .help("Send").accessibilityLabel(Text("Send")).accessibilityIdentifier("dosi.send")
            .transition(.scale(scale: 0.4).combined(with: .opacity))
        } else {
            Image(systemName: "arrow.up").font(.system(size: 12, weight: .bold)).foregroundStyle(theme.colors.inkFaint.opacity(0.7))
                .frame(width: 28, height: 28)
                .background(theme.colors.ink.opacity(0.06), in: Circle())
                .accessibilityHidden(true)
                .transition(.opacity)
        }
    }
}
