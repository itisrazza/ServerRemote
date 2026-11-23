import SwiftUI

/// A dialog as the desktop app draws one: a title, a message or form, and a
/// row of buttons; `leading` ones sit apart on the left.
struct DialogView<Content: View>: View {
    let title: String
    var leading: [DialogButton] = []
    let buttons: [DialogButton]
    var width: CGFloat = 400 * Metrics.scale
    @ViewBuilder let content: Content

    var body: some View {
        VStack(alignment: .leading, spacing: 12 * Metrics.scale) {
            Text(title).font(.uiTitle)
            content
                .font(.ui)
                .fixedSize(horizontal: false, vertical: true)
            HStack(spacing: 8 * Metrics.scale) {
                ForEach(leading) { $0 }
                Spacer(minLength: 0)
                ForEach(buttons) { $0 }
            }
        }
        .foregroundStyle(Color.text)
        .padding(16 * Metrics.scale)
        .frame(maxWidth: width)
        .background(Color.weak, in: RoundedRectangle(cornerRadius: Metrics.panelRadius))
        .overlay(RoundedRectangle(cornerRadius: Metrics.panelRadius).strokeBorder(Color.stronger))
        .shadow(color: .black.opacity(0.5), radius: 12 * Metrics.scale)
        .padding(Metrics.spacing * 4)
    }
}

extension DialogView where Content == Text {
    init(
        title: String,
        message: String,
        leading: [DialogButton] = [],
        buttons: [DialogButton],
        width: CGFloat = 400 * Metrics.scale
    ) {
        self.init(title: title, leading: leading, buttons: buttons, width: width) {
            Text(message)
        }
    }
}

struct DialogButton: View, Identifiable {
    let label: String
    var kind: DialogButtonStyle.Kind = .secondary
    var icon: AppIcon?
    var enabled = true
    let action: () -> Void

    var id: String { label }

    var body: some View {
        Button(action: action) {
            HStack(spacing: Metrics.spacing * 1.5) {
                if let icon { IconView(icon) }
                Text(label)
            }
        }
        .buttonStyle(DialogButtonStyle(kind: kind))
        .disabled(!enabled)
    }
}

/// Shows one dialog at a time over the whole window, dimming the rest.
@MainActor
@Observable
final class DialogPresenter {
    private(set) var content: AnyView?

    func show<V: View>(_ view: V) {
        content = AnyView(view)
    }

    func dismiss() {
        content = nil
    }
}

/// Dims the window behind a dialog.
struct Backdrop<Content: View>: View {
    @ViewBuilder let content: Content

    var body: some View {
        ZStack {
            Color.black.opacity(0.5).ignoresSafeArea()
            content
        }
    }
}
