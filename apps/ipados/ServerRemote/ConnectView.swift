import SwiftUI

/// The Connect page: the desktop's small panel in the middle of the window.
struct ConnectView: View {
    @Environment(AppModel.self) private var model
    @Environment(DialogPresenter.self) private var dialogs
    @State private var request = ConnectRequest.blank

    var body: some View {
        VStack(spacing: Metrics.spacing * 2) {
            ConnectFields(request: $request)
            HStack {
                Button {
                    dialogs.show(NameDialog(request: request))
                } label: {
                    IconView(.save).frame(width: Metrics.controlHeight, height: Metrics.controlHeight)
                }
                .buttonStyle(InsetStyle())
                Spacer()
                DialogButton(label: model.localize("connect-connect"), kind: .primary) {
                    model.core.connect(request: request)
                }
            }
        }
        .padding(Metrics.spacing * 2 * Metrics.scale)
        .frame(width: 300 * Metrics.scale)
        .panel()
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

/// Asks what to call a connection before saving it.
private struct NameDialog: View {
    @Environment(AppModel.self) private var model
    @Environment(DialogPresenter.self) private var dialogs
    @State var request: ConnectRequest

    var body: some View {
        DialogView(
            title: model.localize("connect-save-title"),
            buttons: [
                DialogButton(label: model.localize("button-cancel")) { dialogs.dismiss() },
                DialogButton(label: model.localize("connect-save"), kind: .primary) {
                    model.core.saveNew(request: request)
                    dialogs.dismiss()
                },
            ]
        ) {
            OutlinedField(placeholder: request.hostname, text: $request.name)
        }
    }
}

/// The fields of a connection: a protocol dropdown, then labelled rows.
struct ConnectFields: View {
    @Environment(AppModel.self) private var model
    @Binding var request: ConnectRequest
    var showsName = false

    var body: some View {
        VStack(spacing: Metrics.spacing * 2) {
            ProtocolPicker(selection: $request.protocol)
            if showsName {
                row("connect-name") {
                    OutlinedField(placeholder: "", text: $request.name)
                }
            }
            row("connect-hostname") {
                OutlinedField(placeholder: hostPlaceholder, text: $request.hostname)
                    .keyboardType(.URL)
            }
            if request.protocol == .ilo {
                row("connect-username") {
                    OutlinedField(
                        placeholder: model.localize("connect-username-placeholder"),
                        text: $request.username
                    )
                }
            }
            row("connect-password") {
                OutlinedField(placeholder: "", text: $request.password, secure: true)
            }
        }
    }

    private var hostPlaceholder: String {
        model.localize("connect-hostname-placeholder-port", ["port": defaultPort])
    }

    /// As in `server_remote_app::Protocol::default_port`.
    private var defaultPort: String {
        switch request.protocol {
        case .ilo: "443"
        case .dummy: "0"
        case .vnc, .spice: "5900"
        }
    }

    private func row<Field: View>(_ label: String, @ViewBuilder field: () -> Field) -> some View {
        HStack(spacing: Metrics.spacing * 2) {
            Text(model.localize(label))
                .frame(width: 75 * Metrics.scale, alignment: .leading)
            field()
        }
    }
}

/// The protocol, as a dropdown with each protocol's icon.
private struct ProtocolPicker: View {
    @Binding var selection: ConnectionProtocol

    var body: some View {
        Menu {
            ForEach(protocols(), id: \.self) { kind in
                Button {
                    selection = kind
                } label: {
                    Text(protocolName(protocol: kind))
                }
            }
        } label: {
            HStack(spacing: Metrics.spacing * 1.5) {
                IconView(.connectionProtocol(selection))
                Text(protocolName(protocol: selection))
                Spacer()
                IconView(.chevronDown)
            }
            .padding(.horizontal, 6 * Metrics.scale)
            .frame(height: Metrics.controlHeight)
            .background(Color.base, in: RoundedRectangle(cornerRadius: Metrics.radius))
            .overlay(RoundedRectangle(cornerRadius: Metrics.radius).strokeBorder(Color.strong))
            .contentShape(Rectangle())
        }
        .foregroundStyle(Color.text)
    }
}

extension ConnectRequest {
    static let blank = ConnectRequest(
        protocol: .vnc, name: "", group: "", hostname: "", username: "", password: ""
    )

    init(_ session: Session) {
        self.init(
            protocol: session.protocol,
            name: session.name,
            group: session.group,
            hostname: session.hostname,
            username: session.username,
            password: session.password
        )
    }
}
