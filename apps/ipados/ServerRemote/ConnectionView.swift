import SwiftUI

/// An open connection: its screen fills the page, the connector's menus sit
/// in the toolbar, and its status floats over the bottom edge.
struct ConnectionView: View {
    @Environment(AppModel.self) private var model
    let connection: ConnectionState
    @State private var keyboard = KeyboardFocus()
    @State private var showingInfo = false

    var body: some View {
        content
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .background(Color(.systemBackground))
            .overlay(alignment: .bottom) {
                StatusCapsule(connection: connection)
                    .padding(.bottom, 12)
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
                Label(reason ?? model.localize("status-disconnected"), systemImage: "bolt.horizontal.circle")
            } actions: {
                Button(model.localize("close-tab")) {
                    model.core.closeConnection(connection: connection.id)
                }
                .buttonStyle(.glass)
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
        if case .connected = connection.phase {
            ToolbarItemGroup {
                ForEach(connection.chips, id: \.label) { chip in
                    ChipButton(connection: connection.id, chip: chip)
                }
            }
            ToolbarSpacer(.fixed)
            ToolbarItemGroup {
                inputMenu
                ForEach(contributed, id: \.label) { menu in
                    Menu {
                        actions(menu.actions)
                    } label: {
                        menuLabel(menu)
                    }
                }
            }
            ToolbarSpacer(.fixed)
            ToolbarItemGroup {
                ForEach(connection.links, id: \.url) { link in
                    if let url = URL(string: link.url) {
                        Link(destination: url) {
                            Label {
                                Text(link.label)
                            } icon: {
                                ConnectorIcon(connection: connection.id, icon: link.icon)
                            }
                        }
                    }
                }
                Button(model.localize("menu-virtual-keyboard"), systemImage: "keyboard") {
                    keyboard.toggle()
                }
            }
        }
        ToolbarSpacer(.fixed)
        ToolbarItemGroup {
            Button(model.localize("info-title"), systemImage: "info.circle") { showingInfo.toggle() }
                .popover(isPresented: $showingInfo) {
                    InfoView(connection: connection)
                }
            Button(model.localize("close-tab"), systemImage: "xmark") {
                model.core.closeConnection(connection: connection.id)
            }
        }
    }

    /// The input actions come first in the records, as their own menu.
    private var inputActions: [MenuAction] {
        connection.menus.first { $0.icon == nil }?.actions ?? []
    }

    private var contributed: [ActionMenu] {
        connection.menus.filter { $0.icon != nil }
    }

    private var inputMenu: some View {
        Menu {
            actions(inputActions)
            Section {
                Button(model.localize("trackpad-left"), systemImage: "computermouse") { click(.left) }
                Button(model.localize("trackpad-middle")) { click(.middle) }
                Button(model.localize("trackpad-right")) { click(.right) }
            }
        } label: {
            Label(model.localize("section-input"), systemImage: "computermouse")
        }
    }

    private func menuLabel(_ menu: ActionMenu) -> some View {
        Label {
            Text(menu.label)
        } icon: {
            if let icon = menu.icon {
                ConnectorIcon(connection: connection.id, icon: icon)
            }
        }
    }

    @ViewBuilder
    private func actions(_ actions: [MenuAction]) -> some View {
        ForEach(actions, id: \.id) { action in
            Button {
                model.core.invoke(connection: connection.id, action: action.id)
            } label: {
                if action.checked {
                    Label(action.label, systemImage: "checkmark")
                } else {
                    Text(action.label)
                }
            }
            .disabled(!action.enabled)
        }
    }

    private func click(_ button: MouseButton) {
        model.core.button(connection: connection.id, button: button, pressed: true)
        model.core.button(connection: connection.id, button: button, pressed: false)
    }
}

/// Something ongoing, e.g. a mounted image; tapping it stops it.
private struct ChipButton: View {
    @Environment(AppModel.self) private var model
    let connection: UInt64
    let chip: Chip

    var body: some View {
        Button {
            if let action = chip.closeAction {
                model.core.sendAction(connection: connection, action: action)
            }
        } label: {
            Label {
                Text(chip.label).lineLimit(1)
            } icon: {
                if let icon = chip.icon {
                    ConnectorIcon(connection: connection, icon: icon)
                } else {
                    Image(systemName: "xmark.circle")
                }
            }
            .labelStyle(.titleAndIcon)
        }
        .disabled(chip.closeAction == nil)
    }
}

/// Technical details about the connection.
private struct InfoView: View {
    @Environment(AppModel.self) private var model
    let connection: ConnectionState

    var body: some View {
        Form {
            ForEach(rows, id: \.0) { label, value in
                LabeledContent(label, value: value)
            }
        }
        .frame(minWidth: 340, minHeight: 360)
    }

    private var rows: [(String, String)] {
        let resolution = connection.width.flatMap { width in
            connection.height.map { "\(width)×\($0)" }
        } ?? "—"
        let builtIn = [
            (model.localize("info-protocol"), protocolName(protocol: connection.protocol)),
            (model.localize("info-address"), connection.address),
            (model.localize("info-status"), phaseText(connection.phase, model: model)),
            (model.localize("info-resolution"), resolution),
            (model.localize("info-pointer"), model.localize(connection.absolutePointer ? "pointer-absolute" : "pointer-relative")),
            (model.localize("info-frames"), "\(connection.frames)"),
            (model.localize("info-encryption"), connection.encryption ?? model.localize("encryption-none")),
        ]
        return builtIn + connection.details.map { ($0.label, $0.value) }
    }
}

@MainActor
private func phaseText(_ phase: ConnectionPhase, model: AppModel) -> String {
    switch phase {
    case .connecting: model.localize("status-connecting")
    case .connected: model.localize("status-connected")
    case let .disconnected(reason): reason ?? model.localize("status-disconnected")
    }
}

/// The status, in a glass capsule floating over the screen.
private struct StatusCapsule: View {
    @Environment(AppModel.self) private var model
    let connection: ConnectionState

    var body: some View {
        HStack(spacing: 14) {
            HStack(spacing: 6) {
                Circle().fill(connection.phase.color).frame(width: 8, height: 8)
                Text(phaseText(connection.phase, model: model)).lineLimit(1)
            }
            if let traffic = connection.traffic {
                Label(traffic.received, systemImage: "arrow.down")
                Label(traffic.sent, systemImage: "arrow.up")
            }
            if let encryption = connection.encryption {
                Label(encryption, systemImage: "lock.fill")
            }
            ForEach(connection.status, id: \.text) { entry in
                HStack(spacing: 6) {
                    if let indicator = entry.indicator {
                        Circle().fill(indicator.color).frame(width: 8, height: 8)
                    }
                    if let icon = entry.icon {
                        ConnectorIcon(connection: connection.id, icon: icon, size: 14)
                    }
                    Text(entry.text)
                }
            }
        }
        .font(.footnote)
        .lineLimit(1)
        .padding(.horizontal, 16)
        .padding(.vertical, 10)
        .glassEffect(.regular, in: .capsule)
    }
}

/// The remote screen, scaled to fit. Touches and the pointer move the
/// remote pointer; a touch presses the left button while it lasts.
private struct ScreenView: View {
    @Environment(AppModel.self) private var model
    let connection: ConnectionState
    let screen: CGImage
    let keyboard: KeyboardFocus
    @Environment(\.displayScale) private var displayScale
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
    }

    /// Where the screen sits in `size`, keeping its aspect ratio.
    /// Where the screen sits in `size`, following the Display settings:
    /// fitted (letterboxed, or stretched without keeping the aspect ratio)
    /// or at its actual size, one remote pixel per device pixel.
    private func fitted(in size: CGSize) -> CGRect {
        let display = model.state.settings
        let pixels = CGSize(width: CGFloat(screen.width), height: CGFloat(screen.height))
        let shown: CGSize = switch (display.scaling, display.keepAspectRatio) {
        case (.actual, _):
            CGSize(width: pixels.width / displayScale, height: pixels.height / displayScale)
        case (.fit, false):
            size
        case (.fit, true):
            pixels.applying(.init(scaleX: fitScale(pixels, in: size), y: fitScale(pixels, in: size)))
        }
        return CGRect(
            x: (size.width - shown.width) / 2,
            y: (size.height - shown.height) / 2,
            width: shown.width,
            height: shown.height
        )
    }

    private func fitScale(_ pixels: CGSize, in size: CGSize) -> CGFloat {
        min(size.width / pixels.width, size.height / pixels.height)
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
                Button(model.localize("certificate-always-trust")) {
                    model.core.trustCertificate(connection: id, remember: true)
                }
                Button(model.localize("certificate-connect-once")) {
                    model.core.trustCertificate(connection: id, remember: false)
                }
                Button(model.localize("certificate-block"), role: .destructive) {
                    model.core.blockCertificate(connection: id)
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
                Text(model.localize("certificate-message", ["host": host, "fingerprint": fingerprintLines(fingerprint)]))
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

    /// A fingerprint ("AB:CD:…") on a new line every 8 bytes: it has no
    /// spaces to wrap at.
    private func fingerprintLines(_ fingerprint: String) -> String {
        let bytes = fingerprint.split(separator: ":")
        return stride(from: 0, to: bytes.count, by: 8)
            .map { bytes[$0 ..< min($0 + 8, bytes.count)].joined(separator: ":") }
            .joined(separator: "\n")
    }
}
