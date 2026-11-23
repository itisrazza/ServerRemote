import SwiftUI

/// The Connect page: a form that opens a connection or saves it.
struct ConnectView: View {
    @Environment(AppModel.self) private var model
    @State private var request = ConnectRequest.blank
    @State private var naming = false

    var body: some View {
        Form {
            ConnectFields(request: $request)
        }
        .navigationTitle(model.localize("tab-connect"))
        .toolbar {
            ToolbarItem {
                Button(model.localize("connect-save"), systemImage: "square.and.arrow.down") {
                    naming = true
                }
            }
            ToolbarSpacer(.fixed)
            ToolbarItem {
                Button {
                    model.core.connect(request: request)
                } label: {
                    Text(model.localize("connect-connect")).foregroundStyle(Color.onBrand)
                }
                .buttonStyle(.glassProminent)
            }
        }
        .alert(model.localize("connect-save-title"), isPresented: $naming) {
            TextField(model.localize("connect-name"), text: $request.name)
            Button(model.localize("button-cancel"), role: .cancel) {}
            Button(model.localize("connect-save")) {
                model.core.saveNew(request: request)
            }
        }
    }
}

/// The fields of a connection, for a form on the Connect page or in the
/// manager's editor.
struct ConnectFields: View {
    @Environment(AppModel.self) private var model
    @Binding var request: ConnectRequest

    var body: some View {
        Section {
            Picker(model.localize("info-protocol"), selection: $request.protocol) {
                ForEach(protocols(), id: \.self) { kind in
                    Label {
                        Text(protocolName(protocol: kind))
                    } icon: {
                        IconView(.connectionProtocol(kind))
                    }
                    .tag(kind)
                }
            }
            TextField(
                model.localize("connect-hostname"),
                text: $request.hostname,
                prompt: Text(model.localize("connect-hostname-placeholder-port", ["port": defaultPort]))
            )
            .textInputAutocapitalization(.never)
            .autocorrectionDisabled()
            .keyboardType(.URL)
        }
        Section {
            if request.protocol == .ilo {
                TextField(
                    model.localize("connect-username"),
                    text: $request.username,
                    prompt: Text(model.localize("connect-username-placeholder"))
                )
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
            }
            SecureField(model.localize("connect-password"), text: $request.password)
        }
    }

    /// As in `server_remote_app::Protocol::default_port`.
    private var defaultPort: String {
        switch request.protocol {
        case .ilo: "443"
        case .dummy: "0"
        case .vnc, .spice: "5900"
        }
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
