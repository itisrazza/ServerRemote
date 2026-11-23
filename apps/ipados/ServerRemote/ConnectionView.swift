import SwiftUI

/// An open connection: its screen, the connector's menus, and a status bar.
struct ConnectionView: View {
    @Environment(AppModel.self) private var model
    let connection: ConnectionState
    @State private var keyboard = KeyboardFocus()

    var body: some View {
        VStack(spacing: 0) {
            content
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            Divider()
            StatusBar(connection: connection)
        }
        .navigationTitle(connection.name)
        .navigationBarTitleDisplayMode(.inline)
        .toolbar { toolbar }
        .connectionDialog(connection)
    }

    @ViewBuilder
    private var content: some View {
        switch connection.phase {
        case .connecting:
            ProgressView(model.localize("status-connecting-ellipsis"))
        case let .disconnected(reason):
            ContentUnavailableView {
                Label(reason ?? model.localize("status-disconnected"), systemImage: "bolt.horizontal")
            } actions: {
                Button(model.localize("close-tab")) {
                    model.core.closeConnection(connection: connection.id)
                }
            }
        case .connected:
            if let screen = model.screens[connection.id] {
                ScreenView(connection: connection, screen: screen, keyboard: keyboard)
            } else {
                ProgressView(model.localize("waiting-for-frame"))
            }
        }
    }

    @ToolbarContentBuilder
    private var toolbar: some ToolbarContent {
        ToolbarItemGroup(placement: .primaryAction) {
            ForEach(connection.chips, id: \.label) { chip in
                Button(chip.label, systemImage: "xmark.circle") {
                    if let action = chip.closeAction {
                        model.core.sendAction(connection: connection.id, action: action)
                    }
                }
                .disabled(chip.closeAction == nil)
            }
            ForEach(connection.menus, id: \.label) { menu in
                ActionMenuButton(connection: connection.id, menu: menu)
            }
            Menu(model.localize("section-input"), systemImage: "computermouse") {
                Button(model.localize("trackpad-right")) { click(.right) }
                Button(model.localize("trackpad-middle")) { click(.middle) }
            }
            Button(model.localize("menu-virtual-keyboard"), systemImage: "keyboard") {
                keyboard.toggle()
            }
            ForEach(connection.links, id: \.url) { link in
                if let url = URL(string: link.url) {
                    Link(destination: url) {
                        Label(link.label, systemImage: "arrow.up.forward.square")
                    }
                }
            }
            Button(model.localize("close-tab"), systemImage: "xmark") {
                model.core.closeConnection(connection: connection.id)
            }
        }
    }

    private func click(_ button: MouseButton) {
        model.core.button(connection: connection.id, button: button, pressed: true)
        model.core.button(connection: connection.id, button: button, pressed: false)
    }
}

private struct ActionMenuButton: View {
    @Environment(AppModel.self) private var model
    let connection: UInt64
    let menu: ActionMenu

    var body: some View {
        Menu(menu.label) {
            ForEach(menu.actions, id: \.id) { action in
                Toggle(isOn: Binding(
                    get: { action.checked },
                    set: { _ in model.core.invoke(connection: connection, action: action.id) }
                )) {
                    Text(action.label)
                }
                .disabled(!action.enabled)
            }
        }
    }
}

/// The remote screen, scaled to fit. Touches and the pointer move the
/// remote pointer; a touch presses the left button while it lasts.
private struct ScreenView: View {
    @Environment(AppModel.self) private var model
    let connection: ConnectionState
    let screen: CGImage
    let keyboard: KeyboardFocus
    @State private var pressed = false

    var body: some View {
        GeometryReader { geometry in
            let frame = fitted(in: geometry.size)
            Image(decorative: screen, scale: 1)
                .resizable()
                .interpolation(.none)
                .frame(width: frame.width, height: frame.height)
                .position(x: frame.midX, y: frame.midY)
                .background {
                    KeyCapture(connection: connection.id, focus: keyboard)
                }
                .contentShape(Rectangle())
                .onContinuousHover(coordinateSpace: .local) { phase in
                    if case let .active(point) = phase {
                        move(to: point, in: frame)
                    }
                }
                .gesture(
                    DragGesture(minimumDistance: 0, coordinateSpace: .local)
                        .onChanged { drag in
                            move(to: drag.location, in: frame)
                            if !pressed {
                                pressed = true
                                keyboard.focus()
                                model.core.button(connection: connection.id, button: .left, pressed: true)
                            }
                        }
                        .onEnded { drag in
                            move(to: drag.location, in: frame)
                            pressed = false
                            model.core.button(connection: connection.id, button: .left, pressed: false)
                        }
                )
        }
        .background(.black)
    }

    /// Where the screen sits in `size`, keeping its aspect ratio.
    private func fitted(in size: CGSize) -> CGRect {
        let scale = min(size.width / CGFloat(screen.width), size.height / CGFloat(screen.height))
        let width = CGFloat(screen.width) * scale
        let height = CGFloat(screen.height) * scale
        return CGRect(x: (size.width - width) / 2, y: (size.height - height) / 2, width: width, height: height)
    }

    private func move(to point: CGPoint, in frame: CGRect) {
        guard connection.absolutePointer, frame.width > 0, frame.height > 0 else { return }
        let x = (point.x - frame.minX) / frame.width * CGFloat(screen.width)
        let y = (point.y - frame.minY) / frame.height * CGFloat(screen.height)
        let clampedX = min(max(x, 0), CGFloat(screen.width - 1))
        let clampedY = min(max(y, 0), CGFloat(screen.height - 1))
        model.core.pointer(connection: connection.id, x: Float(clampedX), y: Float(clampedY))
    }
}

private struct StatusBar: View {
    @Environment(AppModel.self) private var model
    let connection: ConnectionState

    var body: some View {
        HStack(spacing: 16) {
            HStack(spacing: 6) {
                PhaseDot(phase: connection.phase)
                Text(phaseLabel)
            }
            if let traffic = connection.traffic {
                Label(traffic.received, systemImage: "arrow.down")
                Label(traffic.sent, systemImage: "arrow.up")
            }
            if let width = connection.width, let height = connection.height {
                Text("\(width)×\(height)")
            }
            if let encryption = connection.encryption {
                Label(encryption, systemImage: "lock")
            }
            ForEach(connection.status, id: \.text) { entry in
                HStack(spacing: 6) {
                    if let indicator = entry.indicator {
                        Circle().fill(indicator.color).frame(width: 8, height: 8)
                    }
                    Text(entry.text)
                }
            }
            Spacer()
        }
        .font(.footnote)
        .lineLimit(1)
        .padding(.horizontal)
        .padding(.vertical, 6)
    }

    private var phaseLabel: String {
        switch connection.phase {
        case .connecting: model.localize("status-connecting-ellipsis")
        case .connected: model.localize("status-connected")
        case .disconnected: model.localize("status-disconnected")
        }
    }
}

private extension Indicator {
    var color: Color {
        switch self {
        case .ok: .green
        case .warning: .yellow
        case .error: .red
        case .inactive: .gray
        }
    }
}

// MARK: - Dialogs

private extension View {
    func connectionDialog(_ connection: ConnectionState) -> some View {
        modifier(ConnectionDialogModifier(connection: connection))
    }
}

private struct ConnectionDialogModifier: ViewModifier {
    @Environment(AppModel.self) private var model
    let connection: ConnectionState

    func body(content: Content) -> some View {
        let dialog = connection.dialog
        let id = connection.id
        let shown = Binding(
            get: { dialog != nil },
            // Each button says what happens; this only catches dismissals.
            set: { _ in }
        )
        content.alert(title(dialog), isPresented: shown, presenting: dialog) { dialog in
            switch dialog {
            case let .confirm(_, _, confirmLabel, destructive):
                Button(model.localize("button-cancel"), role: .cancel) {
                    model.core.closeConnectionDialog(connection: id)
                }
                Button(confirmLabel, role: destructive ? .destructive : nil) {
                    model.core.confirm(connection: id)
                }
            case .trustCertificate:
                Button(model.localize("certificate-block"), role: .destructive) {
                    model.core.blockCertificate(connection: id)
                }
                Button(model.localize("certificate-connect-once")) {
                    model.core.trustCertificate(connection: id, remember: false)
                }
                Button(model.localize("certificate-always-trust")) {
                    model.core.trustCertificate(connection: id, remember: true)
                }
                Button(model.localize("button-cancel"), role: .cancel) {
                    model.core.closeConnection(connection: id)
                }
            case .error:
                Button(model.localize("button-ok"), role: .cancel) {
                    model.core.closeConnectionDialog(connection: id)
                }
            }
        } message: { dialog in
            switch dialog {
            case let .confirm(_, message, _, _), let .error(_, message):
                Text(message)
            case let .trustCertificate(host, fingerprint):
                Text(model.localize("certificate-message", ["host": host, "fingerprint": fingerprint]))
            }
        }
    }

    private func title(_ dialog: ConnectionDialog?) -> String {
        switch dialog {
        case let .confirm(title, _, _, _), let .error(title, _): title
        case .trustCertificate: model.localize("certificate-title")
        case nil: ""
        }
    }
}
