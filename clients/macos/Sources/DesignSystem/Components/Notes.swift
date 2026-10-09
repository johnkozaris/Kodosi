import SwiftUI

struct ProgressCaption: View {
    @Environment(\.theme) private var theme
    let text: String

    var body: some View {
        HStack(spacing: 8) {
            BreathingDot(color: theme.colors.glowOrange)
            Text(text).appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted).shimmer()
        }
    }
}

struct ErrorNote: View {
    @Environment(\.theme) private var theme
    let message: String

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 8) {
            Circle().fill(theme.colors.caution).frame(width: 6, height: 6).offset(y: -1).accessibilityHidden(true)
            Text(message).appTextStyle(.footnote).foregroundStyle(theme.colors.ink).textSelection(.enabled)
                .fixedSize(horizontal: false, vertical: true)
        }
        .padding(.horizontal, 12).padding(.vertical, 9)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(theme.colors.cautionSoft, in: RoundedRectangle(cornerRadius: Radius.md, style: .continuous))
    }
}

struct NoticePill: View {
    @Environment(\.theme) private var theme
    @State private var expanded = false
    let message: String
    var identifier = "notice"
    var retry: (() -> Void)?
    let dismiss: () -> Void

    var body: some View {
        HStack(spacing: 10) {
            Circle().fill(theme.colors.caution).frame(width: 7, height: 7).accessibilityHidden(true)
            Button { expanded.toggle() } label: {
                Text(message).appTextStyle(.footnote).lineLimit(1).truncationMode(.tail)
            }
            .buttonStyle(.plain)
            .popover(isPresented: $expanded, arrowEdge: .bottom) {
                Text(message).appTextStyle(.body).textSelection(.enabled).padding(16).frame(maxWidth: 380)
                    .fixedSize(horizontal: false, vertical: true).popoverSheet()
            }
            if let retry {
                Button("Try again", action: retry).buttonStyle(.kodosi(.tinted, size: .small))
            }
            IconButton(title: "Dismiss", symbol: "xmark", identifier: "\(identifier).dismiss", size: 22, action: dismiss)
        }
        .foregroundStyle(theme.colors.ink)
        .padding(.leading, 14).padding(.trailing, 6).frame(height: 36)
        .frame(maxWidth: 520)
        .fixedSize(horizontal: true, vertical: false)
        .raisedCapsule(fill: theme.colors.lifted, elevation: .floating)
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(identifier)
    }
}

struct EmptyState<Art: View, Actions: View>: View {
    @Environment(\.theme) private var theme
    let title: LocalizedStringKey
    var message: LocalizedStringKey?
    @ViewBuilder let art: () -> Art
    @ViewBuilder let actions: () -> Actions

    var body: some View {
        VStack(spacing: 18) {
            art().arrive()
            VStack(spacing: 6) {
                Text(title).appTextStyle(.headline).foregroundStyle(theme.colors.ink)
                if let message {
                    Text(message).appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted)
                        .multilineTextAlignment(.center).frame(maxWidth: 300)
                }
            }.arrive(delay: 0.05)
            HStack(spacing: 8) { actions() }.arrive(delay: 0.1)
        }
        .padding(28)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

struct SkeletonBlock: View {
    @Environment(\.theme) private var theme
    @State private var sweeping = false
    var width: CGFloat?
    var height: CGFloat = 10
    var radius: CGFloat = 5

    var body: some View {
        RoundedRectangle(cornerRadius: radius, style: .continuous)
            .fill(theme.colors.ink.opacity(0.07))
            .frame(width: width, height: height)
            .frame(maxWidth: width == nil ? .infinity : nil)
            .overlay {
                if !theme.motion.reduced {
                    GeometryReader { geometry in
                        LinearGradient(colors: [.clear, theme.colors.ink.opacity(0.08), .clear], startPoint: .leading, endPoint: .trailing)
                            .frame(width: geometry.size.width * 0.6)
                            .offset(x: sweeping ? geometry.size.width : -geometry.size.width * 0.6)
                    }
                    .clipShape(RoundedRectangle(cornerRadius: radius, style: .continuous))
                }
            }
            .onAppear {
                guard !theme.motion.reduced else { return }
                withAnimation(.easeInOut(duration: 1.5).repeatForever(autoreverses: false)) { sweeping = true }
            }
    }
}

struct DashedSlot<Content: View>: View {
    @Environment(\.theme) private var theme
    @State private var hovered = false
    var radius: CGFloat = Radius.xl
    @ViewBuilder let content: () -> Content

    var body: some View {
        content()
            .background {
                RoundedRectangle(cornerRadius: radius, style: .continuous)
                    .fill(theme.colors.accent.opacity(hovered ? 0.07 : 0))
            }
            .overlay {
                RoundedRectangle(cornerRadius: radius, style: .continuous)
                    .strokeBorder(
                        hovered ? theme.colors.accent.opacity(0.7) : theme.colors.inkFaint.opacity(0.5),
                        style: StrokeStyle(lineWidth: 1.2, dash: [5, 5])
                    )
            }
            .contentShape(RoundedRectangle(cornerRadius: radius, style: .continuous))
            .onHover { hovered = $0 }
            .animation(theme.motion.hover, value: hovered)
    }
}
