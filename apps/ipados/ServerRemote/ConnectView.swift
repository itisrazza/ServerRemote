import SwiftUI

/// The Connect page: a form that opens a connection or saves it.
struct ConnectView: View {
    @Environment(AppModel.self) private var model
    @State private var request = ConnectRequest.blank
    @State private var naming = false

    var body: some View {
        Form { ConnectFields(request: $request) }
            .navigationTitle(model.localize("tab-connect"))
            .toolbar {
                ToolbarItemGroup(placement: .primaryAction) {
                    Button(model.localize("connect-save"), systemImage: "square.and.arrow.down") {
                        naming = true
                    }
                    Button(model.localize("connect-connect")) {
                        model.core.connect(request: request)
                    }
                    .buttonStyle(.borderedProminent)
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

/// The fields of a connection, as on the Connect page and in the manager.
struct ConnectFields: View {
    @Environment(AppModel.self) private var model
    @Binding var request: ConnectRequest

    var body: some View {
        Group {
            Picker(model.localize("info-protocol"), selection: $request.protocol) {
                ForEach(protocols(), id: \.self) { kind in
                    Text(protocolName(protocol: kind)).tag(kind)
                }
            }
            TextField(
                model.localize("connect-hostname"),
                text: $request.hostname,
                prompt: Text(model.localize("connect-hostname-placeholder"))
            )
            .textInputAutocapitalization(.never)
            .autocorrectionDisabled()
            .keyboardType(.URL)
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
