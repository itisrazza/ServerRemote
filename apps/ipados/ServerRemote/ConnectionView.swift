import SwiftUI

/// An open connection, laid out as on the desktop: a toolbar, the screen,
/// and a status bar, with the connection's dialogs over them.
struct ConnectionView: View {
    @Environment(AppModel.self) private var model
    let connection: ConnectionState
    @State private var keyboard = KeyboardFocus()

    var body: some View {
        VStack(spacing: Metrics.spacing) {
            if !isDisconnected {
                Toolbar(connection: connection, keyboard: keyboard)
            }
            content
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            StatusBar(connection: connection)
        }
        .overlay {
            if let dialog = connection.dialog {
                Backdrop { ConnectionDialogView(connection: connection, dialog: dialog) }
            }
        }
    }

    private var isDisconnected: Bool {
        if case .disconnected = connection.phase { true } else { false }
    }

    @ViewBuilder
    private var content: some View {
        switch connection.phase {
        case .connecting:
            Text(model.localize("status-connecting-ellipsis"))
        case let .disconnected(reason):
            VStack(spacing: 8 * Metrics.scale) {
                Text(reason ?? model.localize("status-disconnected"))
                    .multilineTextAlignment(.center)
                DialogButton(label: model.localize("close-tab")) {
                    model.core.closeConnection(connection: connection.id)
                }
            }
            .padding()
        case .connected:
            if let screen = model.screens[connection.id] {
                ScreenView(connection: connection, screen: screen, keyboard: keyboard)
            } else {
                Text(model.localize("waiting-for-frame"))
            }
        }
    }
}

/// The row above the screen: the input menu, the connector's menus and
/// chips, then its links and the info popout.
private struct Toolbar: View {
    @Environment(AppModel.self) private var model
    let connection: ConnectionState
    let keyboard: KeyboardFocus
    @State private var showingInfo = false

    var body: some View {
        HStack(spacing: Metrics.spacing) {
            inputMenu
            if !contributed.isEmpty {
                VerticalDivider()
                ForEach(contributed, id: \.label) { menu in
                    MenuButton(icon: menuIcon(menu)) { actions(menu.actions) }
                }
            }
            ForEach(connection.chips, id: \.label) { chip in
                ChipView(connection: connection.id, chip: chip)
            }
            Spacer(minLength: 0)
            VerticalDivider()
            ForEach(connection.links, id: \.url) { link in
                if let url = URL(string: link.url) {
                    Link(destination: url) {
                        IconView(.connector(connection: connection.id, id: link.icon))
                            .frame(width: Metrics.controlHeight, height: Metrics.controlHeight)
                    }
                    .buttonStyle(BorderedStyle())
                }
            }
            Button {
                showingInfo.toggle()
            } label: {
                IconView(.information).frame(width: Metrics.controlHeight, height: Metrics.controlHeight)
            }
            .buttonStyle(BorderedStyle(active: showingInfo))
            .popover(isPresented: $showingInfo) {
                InfoView(connection: connection)
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

    private func menuIcon(_ menu: ActionMenu) -> IconView.Source {
        menu.icon.map { .connector(connection: connection.id, id: $0) } ?? .app(.keyboard)
    }

    private var inputMenu: some View {
        MenuButton(icon: .app(.keyboard)) {
            actions(inputActions)
            if !inputActions.isEmpty { Divider() }
            Button(model.localize("menu-virtual-keyboard")) { keyboard.toggle() }
            Divider()
            Button(model.localize("trackpad-right")) { click(.right) }
            Button(model.localize("trackpad-middle")) { click(.middle) }
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

/// A toolbar menu as the desktop draws one: an outlined icon and chevron.
private struct MenuButton<Items: View>: View {
    let icon: IconView.Source
    @ViewBuilder let items: Items

    var body: some View {
        Menu {
            items
        } label: {
            HStack(spacing: 2 * Metrics.scale) {
                IconView(icon)
                IconView(.chevronDown, size: Metrics.iconSize * 0.75)
            }
            .padding(.horizontal, 6 * Metrics.scale)
            .frame(height: Metrics.controlHeight)
            .overlay(RoundedRectangle(cornerRadius: Metrics.radius).strokeBorder(Color.strong))
            .contentShape(Rectangle())
        }
        .foregroundStyle(Color.text)
    }
}

/// Something ongoing, e.g. a mounted image, with a button to stop it.
private struct ChipView: View {
    @Environment(AppModel.self) private var model
    let connection: UInt64
    let chip: Chip

    var body: some View {
        HStack(spacing: Metrics.spacing) {
            if let icon = chip.icon {
                IconView(.connector(connection: connection, id: icon))
            }
            Text(chip.label).lineLimit(1).frame(maxWidth: 160 * Metrics.scale, alignment: .leading)
            if let action = chip.closeAction {
                Button {
                    model.core.sendAction(connection: connection, action: action)
                } label: {
                    IconView(.close, size: Metrics.iconSize * 0.9)
                }
                .buttonStyle(.plain)
            }
        }
        .padding(.horizontal, 6 * Metrics.scale)
        .frame(height: Metrics.controlHeight)
        .background(Color.strong, in: RoundedRectangle(cornerRadius: Metrics.radius))
        .overlay(RoundedRectangle(cornerRadius: Metrics.radius).strokeBorder(Color.strongest))
    }
}

/// Technical details about the connection.
private struct InfoView: View {
    @Environment(AppModel.self) private var model
    let connection: ConnectionState

    var body: some View {
        Grid(alignment: .leading, horizontalSpacing: 16 * Metrics.scale, verticalSpacing: 6 * Metrics.scale) {
            ForEach(rows, id: \.0) { label, value in
                GridRow {
                    Text(label).foregroundStyle(Color.strongest)
                    Text(value)
                }
            }
        }
        .font(.ui)
        .foregroundStyle(Color.text)
        .padding(12 * Metrics.scale)
        .frame(minWidth: 280 * Metrics.scale)
        .presentationBackground(Color.weak)
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

/// The row below the screen, in groups split by dividers.
private struct StatusBar: View {
    @Environment(AppModel.self) private var model
    let connection: ConnectionState

    var body: some View {
        HStack(spacing: 12 * Metrics.scale) {
            ForEach(Array(groups.enumerated()), id: \.offset) { index, group in
                if index > 0 { VerticalDivider(height: 16 * Metrics.scale) }
                group
            }
            Spacer(minLength: 0)
        }
        .lineLimit(1)
        .padding(.horizontal, 8 * Metrics.scale)
        .frame(height: Metrics.controlHeight)
    }

    private var groups: [AnyView] {
        var groups = [AnyView(phase)]
        if let traffic = connection.traffic {
            groups.append(AnyView(HStack(spacing: 6 * Metrics.scale) {
                IconView(.download)
                Text(traffic.received)
                IconView(.upload).padding(.leading, 6 * Metrics.scale)
                Text(traffic.sent)
            }))
        }
        if let width = connection.width, let height = connection.height {
            groups.append(AnyView(Text("\(width)×\(height)")))
        }
        if let encryption = connection.encryption {
            groups.append(AnyView(HStack(spacing: 6 * Metrics.scale) {
                IconView(.locked)
                Text(encryption)
            }))
        }
        if !connection.status.isEmpty {
            groups.append(AnyView(HStack(spacing: 16 * Metrics.scale) {
                ForEach(connection.status, id: \.text) { entry in
                    HStack(spacing: 6 * Metrics.scale) {
                        if let indicator = entry.indicator {
                            StatusDot(color: indicator.color)
                        }
                        if let icon = entry.icon {
                            IconView(.connector(connection: connection.id, id: icon))
                        }
                        Text(entry.text)
                    }
                }
            }))
        }
        return groups
    }

    private var phase: some View {
        HStack(spacing: 6 * Metrics.scale) {
            StatusDot(color: phaseColor)
            Text(phaseLabel)
        }
    }

    private var phaseColor: Color {
        switch connection.phase {
        case .connecting: .warning
        case .connected: .success
        case .disconnected: .danger
        }
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
        case .ok: .success
        case .warning: .warning
        case .error: .danger
        case .inactive: .strongest
        }
    }
}

/// A dialog the connection put up.
private struct ConnectionDialogView: View {
    @Environment(AppModel.self) private var model
    let connection: ConnectionState
    let dialog: ConnectionDialog

    var body: some View {
        let id = connection.id
        switch dialog {
        case let .confirm(title, message, confirmLabel, destructive):
            DialogView(title: title, message: message, buttons: [
                DialogButton(label: model.localize("button-cancel")) {
                    model.core.closeConnectionDialog(connection: id)
                },
                DialogButton(label: confirmLabel, kind: destructive ? .destructive : .primary) {
                    model.core.confirm(connection: id)
                },
            ])
        case let .trustCertificate(host, fingerprint):
            DialogView(
                title: model.localize("certificate-title"),
                message: model.localize(
                    "certificate-message",
                    ["host": host, "fingerprint": fingerprintLines(fingerprint)]
                ),
                leading: [
                    DialogButton(label: model.localize("certificate-block")) {
                        model.core.blockCertificate(connection: id)
                    },
                ],
                buttons: [
                    DialogButton(label: model.localize("button-cancel")) {
                        model.core.closeConnection(connection: id)
                    },
                    DialogButton(label: model.localize("certificate-connect-once")) {
                        model.core.trustCertificate(connection: id, remember: false)
                    },
                    DialogButton(label: model.localize("certificate-always-trust"), kind: .primary) {
                        model.core.trustCertificate(connection: id, remember: true)
                    },
                ],
                width: 460 * Metrics.scale
            )
        case let .error(title, message):
            DialogView(title: title, message: message, buttons: [
                DialogButton(label: model.localize("button-ok"), kind: .primary) {
                    model.core.closeConnectionDialog(connection: id)
                },
            ])
        }
    }

    /// A fingerprint ("AB:CD:…") on a new line every 8 bytes, as on the
    /// desktop: it has no spaces to wrap at.
    private func fingerprintLines(_ fingerprint: String) -> String {
        let bytes = fingerprint.split(separator: ":")
        return stride(from: 0, to: bytes.count, by: 8)
            .map { bytes[$0 ..< min($0 + 8, bytes.count)].joined(separator: ":") }
            .joined(separator: "\n")
    }
}
