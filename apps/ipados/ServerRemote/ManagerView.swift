import SwiftUI

/// Saved connections as cards, filtered by group. Tap a card to connect;
/// hold it for more.
struct ManagerView: View {
    @Environment(AppModel.self) private var model
    /// The group shown, or `nil` for all of them.
    @State private var group: String?
    @State private var editing: Editing?
    @State private var deleting: Session?
    @State private var grouping: Session?
    @State private var newGroup = ""

    var body: some View {
        content
            .navigationTitle(title)
            .toolbar { toolbar }
            .sheet(item: $editing) { EditSessionSheet(editing: $0) }
            .confirmationDialog(
                model.localize("manager-delete-title", ["name": deleting?.name ?? ""]),
                isPresented: isShown($deleting),
                titleVisibility: .visible,
                presenting: deleting
            ) { session in
                Button(model.localize("manager-delete-confirm"), role: .destructive) {
                    model.core.deleteSession(session: session.id)
                }
            } message: { _ in
                Text(model.localize("manager-delete-message"))
            }
            .alert(model.localize("manager-new-group-title"), isPresented: isShown($grouping), presenting: grouping) { session in
                TextField(model.localize("manager-group-name"), text: $newGroup)
                Button(model.localize("button-cancel"), role: .cancel) {}
                Button(model.localize("manager-new-group-create")) {
                    let name = newGroup.trimmingCharacters(in: .whitespaces)
                    if !name.isEmpty {
                        model.core.setGroup(session: session.id, group: name)
                    }
                    newGroup = ""
                }
            }
    }

    /// The chosen group, unless its last session has gone since.
    private var shownGroup: String? {
        group.flatMap { name in model.state.groups.contains { $0.name == name } ? name : nil }
    }

    private var title: String {
        shownGroup ?? model.localize("tab-manager")
    }

    @ViewBuilder
    private var content: some View {
        let sessions = model.state.sessions.filter { shownGroup == nil || $0.group == shownGroup }
        if model.state.sessions.isEmpty {
            ContentUnavailableView(model.localize("manager-empty"), systemImage: "square.grid.2x2")
        } else {
            ScrollView {
                LazyVGrid(columns: [GridItem(.adaptive(minimum: 240), spacing: 20)], spacing: 20) {
                    ForEach(sessions, id: \.id) { session in
                        SessionCard(
                            session: session,
                            edit: { editing = Editing(id: session.id, request: ConnectRequest(session)) },
                            newGroup: { grouping = session },
                            delete: { deleting = session }
                        )
                    }
                }
                .padding(20)
            }
            .background(Color(.systemGroupedBackground))
        }
    }

    @ToolbarContentBuilder
    private var toolbar: some ToolbarContent {
        ToolbarItem {
            Menu {
                Picker(model.localize("manager-group-menu"), selection: $group) {
                    Text(model.localize("manager-all", ["count": "\(model.state.sessions.count)"]))
                        .tag(String?.none)
                    ForEach(model.state.groups, id: \.name) { entry in
                        Text(model.localize("manager-group", ["group": entry.name, "count": "\(entry.count)"]))
                            .tag(Optional(entry.name))
                    }
                }
            } label: {
                Label(model.localize("manager-group-menu"), systemImage: "line.3.horizontal.decrease")
            }
        }
        ToolbarSpacer(.fixed)
        ToolbarItemGroup {
            Button(model.localize("manager-import"), systemImage: "square.and.arrow.down") {
                model.core.importSessions()
            }
            Button(model.localize("manager-add"), systemImage: "plus") {
                var request = ConnectRequest.blank
                request.group = shownGroup ?? ""
                editing = Editing(id: nil, request: request)
            }
        }
    }

    private func isShown<T>(_ item: Binding<T?>) -> Binding<Bool> {
        Binding(get: { item.wrappedValue != nil }, set: { if !$0 { item.wrappedValue = nil } })
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
    let edit: () -> Void
    let newGroup: () -> Void
    let delete: () -> Void

    var body: some View {
        Button {
            model.core.openSaved(session: session.id)
        } label: {
            VStack(alignment: .leading, spacing: 0) {
                thumbnail
                    .frame(maxWidth: .infinity)
                    .aspectRatio(16 / 9, contentMode: .fit)
                    .clipped()
                VStack(alignment: .leading, spacing: 2) {
                    Text(session.name).font(.headline).lineLimit(1)
                    Text(session.hostname).font(.subheadline).foregroundStyle(.secondary).lineLimit(1)
                    Text(protocolName(protocol: session.protocol)).font(.caption).foregroundStyle(.secondary)
                }
                .padding(12)
            }
            .background(Color(.secondarySystemGroupedBackground))
            .clipShape(RoundedRectangle(cornerRadius: 16, style: .continuous))
            .contentShape(.contextMenuPreview, RoundedRectangle(cornerRadius: 16, style: .continuous))
        }
        .buttonStyle(.plain)
        .contextMenu { menu }
    }

    @ViewBuilder
    private var thumbnail: some View {
        if let png = model.core.thumbnail(session: session.id), let image = UIImage(data: png) {
            Image(uiImage: image).resizable().scaledToFill()
        } else {
            ZStack {
                Color(.tertiarySystemFill)
                IconView(.connectionProtocol(session.protocol), size: 56)
                    .foregroundStyle(.tertiary)
            }
        }
    }

    @ViewBuilder
    private var menu: some View {
        Button(model.localize("connect-connect"), systemImage: "play") {
            model.core.openSaved(session: session.id)
        }
        Button(model.localize("manager-edit"), systemImage: "pencil", action: edit)
        Menu {
            Button(model.localize("manager-group-none")) {
                model.core.setGroup(session: session.id, group: "")
            }
            ForEach(model.state.groups, id: \.name) { group in
                Button(group.name) {
                    model.core.setGroup(session: session.id, group: group.name)
                }
            }
            Divider()
            Button(model.localize("manager-new-group"), systemImage: "folder.badge.plus", action: newGroup)
        } label: {
            Label(model.localize("manager-group-menu"), systemImage: "folder")
        }
        Button(model.localize("manager-export"), systemImage: "square.and.arrow.up") {
            model.core.exportSession(session: session.id)
        }
        Divider()
        Button(model.localize("manager-delete"), systemImage: "trash", role: .destructive, action: delete)
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
                }
                ConnectFields(request: $editing.request)
            }
            .navigationTitle(model.localize(editing.id == nil ? "manager-add-title" : "manager-edit-title"))
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button(model.localize("button-cancel"), role: .cancel) { dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button(model.localize("connect-save"), role: .confirm) {
                        model.core.saveSession(editing: editing.id, request: editing.request)
                        dismiss()
                    }
                }
            }
        }
    }
}
