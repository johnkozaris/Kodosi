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
                .background { BezelBackground(shape: Capsule(), fill: theme.colors.raised, bezel: .quiet) }
                .transition(AnyTransition.rise)
            }
            HStack(alignment: .bottom, spacing: 4) {
                LookingDosiFace(mood: dosi.mood(listening: focused.wrappedValue), follows: !dosi.busy && !focused.wrappedValue)
                    .fold(FoldPart.face)
                    .frame(width: 30, height: 30)
                TextField("Ask Dosi", text: $dosi.draft, axis: .vertical)
                    .lineLimit(1 ... 8).textFieldStyle(.plain).appTextStyle(.callout).focused(focused)
                    .padding(.vertical, 7)
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
                    .arrive(delay: 0.12, rise: 0)
                action.frame(width: 30, height: 30)
            }
            .padding(5)
            .background {
                RaisedBackground(shape: shape, fill: theme.colors.lifted, elevation: focused.wrappedValue ? .lifted : .resting)
                    .fold(FoldPart.box)
            }
            .overlay {
                shape.strokeBorder(theme.colors.accent.mix(with: theme.colors.lifted, by: 0.5), lineWidth: 1.5)
                    .opacity(focused.wrappedValue ? 1 : 0)
            }
            .contentShape(shape)
            .onTapGesture { focused.wrappedValue = true }
        }
        .padding(.horizontal, 10).padding(.bottom, 10).padding(.top, 6)
        .animation(theme.motion.snappy, value: typed)
        .animation(theme.motion.snappy, value: stops)
        .animation(theme.motion.spring, value: dosi.queued.map(\.id))
        .animation(theme.motion.hover, value: focused.wrappedValue)
    }

    @ViewBuilder
    private var action: some View {
        if stops {
            Button { deps.dosi.stop() } label: {
                RoundedRectangle(cornerRadius: 2, style: .continuous).fill(theme.colors.ink).frame(width: 9, height: 9)
                    .frame(width: 30, height: 30)
                    .background { BezelBackground(shape: Circle(), fill: theme.buttonFace(), bezel: .quiet) }
            }
            .buttonStyle(PressScaleStyle())
            .tip("Stop (⌘.)").accessibilityLabel(Text("Stop")).accessibilityIdentifier("dosi.stop")
            .transition(AnyTransition.pop)
        } else {
            Button { deps.sendToDosi() } label: {
                Image(systemName: "arrow.up").font(.system(size: 13, weight: .bold))
                    .foregroundStyle(typed ? theme.colors.onAccent : theme.colors.inkFaint)
                    .frame(width: 30, height: 30)
                    .background {
                        if typed {
                            BezelBackground(shape: Circle(), fill: theme.colors.accent).transition(.opacity)
                        } else {
                            Circle().fill(theme.colors.lifted.mix(with: theme.colors.ink, by: 0.08))
                        }
                    }
            }
            .buttonStyle(PressScaleStyle()).disabled(!typed)
            .tip("Send").accessibilityLabel(Text("Send")).accessibilityIdentifier("dosi.send")
            .transition(AnyTransition.pop)
        }
    }
}
