import SwiftUI

/// Saved connections as cards: tap one to connect, hold it for more.
struct ManagerView: View {
    @Environment(AppModel.self) private var model
    @State private var editing: Editing?
    @State private var deleting: Session?

    var body: some View {
        Group {
            if model.state.sessions.isEmpty {
                ContentUnavailableView(model.localize("manager-empty"), systemImage: "square.grid.2x2")
            } else {
                ScrollView {
                    LazyVGrid(columns: [GridItem(.adaptive(minimum: 220), spacing: 16)], spacing: 16) {
                        ForEach(model.state.sessions, id: \.id) { session in
                            SessionCard(session: session, editing: $editing, deleting: $deleting)
                        }
                    }
                    .padding()
                }
            }
        }
        .navigationTitle(model.localize("tab-manager"))
        .toolbar {
            ToolbarItemGroup(placement: .primaryAction) {
                Button(model.localize("manager-import"), systemImage: "square.and.arrow.down") {
                    model.core.importSessions()
                }
                Button(model.localize("manager-add"), systemImage: "plus") {
                    editing = Editing(id: nil, request: .blank)
                }
            }
        }
        .sheet(item: $editing) { editing in
            EditSessionSheet(editing: editing)
        }
        .alert(
            model.localize("manager-delete-title", ["name": deleting?.name ?? ""]),
            isPresented: Binding(get: { deleting != nil }, set: { if !$0 { deleting = nil } }),
            presenting: deleting
        ) { session in
            Button(model.localize("button-cancel"), role: .cancel) {}
            Button(model.localize("manager-delete-confirm"), role: .destructive) {
                model.core.deleteSession(session: session.id)
            }
        } message: { _ in
            Text(model.localize("manager-delete-message"))
        }
    }
}

/// A session being added (`id` unset) or edited.
struct Editing: Identifiable {
    let id: UInt64?
    var request: ConnectRequest
}

private struct SessionCard: View {
    @Environment(AppModel.self) private var model
    let session: Session
    @Binding var editing: Editing?
    @Binding var deleting: Session?

    var body: some View {
        Button {
            model.core.openSaved(session: session.id)
        } label: {
            VStack(alignment: .leading, spacing: 4) {
                thumbnail
                    .frame(maxWidth: .infinity)
                    .aspectRatio(16 / 9, contentMode: .fit)
                    .background(.quaternary)
                    .clipShape(RoundedRectangle(cornerRadius: 6))
                Text(session.name).font(.headline)
                Text(session.hostname).foregroundStyle(.secondary)
                Text(protocolName(protocol: session.protocol)).foregroundStyle(.secondary)
            }
            .padding(8)
            .background(.background.secondary, in: RoundedRectangle(cornerRadius: 10))
        }
        .buttonStyle(.plain)
        .contextMenu {
            Button(model.localize("connect-connect"), systemImage: "play") {
                model.core.openSaved(session: session.id)
            }
            Button(model.localize("manager-edit"), systemImage: "pencil") {
                editing = Editing(id: session.id, request: ConnectRequest(session))
            }
            Button(model.localize("manager-export"), systemImage: "square.and.arrow.up") {
                model.core.exportSession(session: session.id)
            }
            Button(model.localize("manager-delete"), systemImage: "trash", role: .destructive) {
                deleting = session
            }
        }
    }

    @ViewBuilder
    private var thumbnail: some View {
        if let png = model.core.thumbnail(session: session.id), let image = UIImage(data: png) {
            Image(uiImage: image).resizable().scaledToFill()
        } else {
            Text(protocolName(protocol: session.protocol))
                .font(.title.weight(.heavy))
                .foregroundStyle(.secondary)
        }
    }
}

private struct EditSessionSheet: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State var editing: Editing

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    TextField(model.localize("connect-name"), text: $editing.request.name)
                    TextField(model.localize("manager-group-menu"), text: $editing.request.group)
                }
                Section {
                    ConnectFields(request: $editing.request)
                }
            }
            .navigationTitle(model.localize(editing.id == nil ? "manager-add-title" : "manager-edit-title"))
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button(model.localize("button-cancel")) { dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button(model.localize("connect-save")) {
                        model.core.saveSession(editing: editing.id, request: editing.request)
                        dismiss()
                    }
                }
            }
        }
    }
}
