import SwiftUI

/// Saved connections as cards in a panel, filtered by the group chosen in
/// the sidebar beside it. Tap a card to connect; hold it for more.
struct ManagerView: View {
    @Environment(AppModel.self) private var model
    @Environment(DialogPresenter.self) private var dialogs
    /// The group shown, or `nil` for all of them.
    @State private var group: String?

    var body: some View {
        HStack(alignment: .top, spacing: Metrics.spacing * 2) {
            sidebar
                .frame(width: 200 * Metrics.scale)
            content
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
                .panel()
        }
    }

    /// The chosen group, unless its last session has gone since.
    private var shownGroup: String? {
        group.flatMap { name in model.state.groups.contains { $0.name == name } ? name : nil }
    }

    private var sidebar: some View {
        VStack(spacing: Metrics.spacing) {
            groupItem(
                model.localize("manager-all", ["count": "\(model.state.sessions.count)"]),
                group: nil
            )
            ForEach(model.state.groups, id: \.name) { entry in
                groupItem(
                    model.localize("manager-group", ["group": entry.name, "count": "\(entry.count)"]),
                    group: entry.name
                )
            }
            Spacer()
        }
    }

    private func groupItem(_ label: String, group: String?) -> some View {
        Button(label) { self.group = group }
            .buttonStyle(SidebarItemStyle(active: shownGroup == group))
    }

    private var content: some View {
        VStack(alignment: .leading, spacing: Metrics.spacing * 2) {
            HStack(spacing: Metrics.spacing) {
                toolbarButton(model.localize("manager-add"), icon: .add) {
                    var request = ConnectRequest.blank
                    request.group = shownGroup ?? ""
                    dialogs.show(EditDialog(editing: nil, request: request))
                }
                toolbarButton(model.localize("manager-import"), icon: nil) {
                    model.core.importSessions()
                }
            }
            let sessions = model.state.sessions.filter { shownGroup == nil || $0.group == shownGroup }
            if model.state.sessions.isEmpty {
                Text(model.localize("manager-empty"))
                    .foregroundStyle(Color.strongest)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                ScrollView {
                    LazyVGrid(
                        columns: [GridItem(.adaptive(minimum: 220 * Metrics.scale), spacing: Metrics.spacing * 2)],
                        spacing: Metrics.spacing * 2
                    ) {
                        ForEach(sessions, id: \.id) { session in
                            SessionCard(session: session)
                        }
                    }
                }
            }
        }
        .padding(Metrics.spacing * 2)
    }

    private func toolbarButton(_ label: String, icon: AppIcon?, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            HStack(spacing: Metrics.spacing * 1.5) {
                if let icon { IconView(icon) }
                Text(label)
            }
            .padding(.horizontal, 10 * Metrics.scale)
            .frame(height: Metrics.controlHeight)
        }
        .buttonStyle(InsetStyle())
    }
}

private struct SessionCard: View {
    @Environment(AppModel.self) private var model
    @Environment(DialogPresenter.self) private var dialogs
    let session: Session

    var body: some View {
        Button {
            model.core.openSaved(session: session.id)
        } label: {
            VStack(alignment: .leading, spacing: 2 * Metrics.scale) {
                thumbnail
                    .frame(maxWidth: .infinity)
                    .aspectRatio(16 / 9, contentMode: .fit)
                    .clipShape(RoundedRectangle(cornerRadius: 4 * Metrics.scale))
                    .padding(.bottom, Metrics.spacing)
                Text(session.name).font(.uiBold).lineLimit(1)
                Text(session.hostname).foregroundStyle(Color.strongest).lineLimit(1)
                Text(protocolName(protocol: session.protocol)).foregroundStyle(Color.strongest)
            }
            .padding(Metrics.spacing * 2)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .buttonStyle(InsetStyle(radius: Metrics.panelRadius))
        .contextMenu { menu }
    }

    @ViewBuilder
    private var thumbnail: some View {
        if let png = model.core.thumbnail(session: session.id), let image = UIImage(data: png) {
            Image(uiImage: image).resizable().scaledToFill()
        } else {
            ZStack {
                Color.neutral
                IconView(.connectionProtocol(session.protocol), size: 64 * Metrics.scale)
                    .foregroundStyle(Color.strongest)
                    .opacity(0.6)
            }
        }
    }

    @ViewBuilder
    private var menu: some View {
        Button(model.localize("connect-connect")) {
            model.core.openSaved(session: session.id)
        }
        Divider()
        Button(model.localize("manager-edit")) {
            dialogs.show(EditDialog(editing: session.id, request: ConnectRequest(session)))
        }
        Menu(model.localize("manager-group-menu")) {
            Button(model.localize("manager-group-none")) {
                model.core.setGroup(session: session.id, group: "")
            }
            ForEach(model.state.groups, id: \.name) { group in
                Button(group.name) {
                    model.core.setGroup(session: session.id, group: group.name)
                }
            }
            Divider()
            Button(model.localize("manager-new-group")) {
                dialogs.show(NewGroupDialog(session: session.id))
            }
        }
        Divider()
        Button(model.localize("manager-export")) {
            model.core.exportSession(session: session.id)
        }
        Button(model.localize("manager-delete"), role: .destructive) {
            dialogs.show(DeleteDialog(session: session))
        }
    }
}

/// Adds a session (`editing` unset) or edits one.
private struct EditDialog: View {
    @Environment(AppModel.self) private var model
    @Environment(DialogPresenter.self) private var dialogs
    let editing: UInt64?
    @State var request: ConnectRequest

    var body: some View {
        DialogView(
            title: model.localize(editing == nil ? "manager-add-title" : "manager-edit-title"),
            buttons: [
                DialogButton(label: model.localize("button-cancel")) { dialogs.dismiss() },
                DialogButton(label: model.localize("connect-save"), kind: .primary) {
                    model.core.saveSession(editing: editing, request: request)
                    dialogs.dismiss()
                },
            ],
            width: 320 * Metrics.scale
        ) {
            ConnectFields(request: $request, showsName: true)
        }
    }
}

private struct NewGroupDialog: View {
    @Environment(AppModel.self) private var model
    @Environment(DialogPresenter.self) private var dialogs
    let session: UInt64
    @State private var name = ""

    var body: some View {
        DialogView(
            title: model.localize("manager-new-group-title"),
            buttons: [
                DialogButton(label: model.localize("button-cancel")) { dialogs.dismiss() },
                DialogButton(
                    label: model.localize("manager-new-group-create"),
                    kind: .primary,
                    enabled: !name.trimmingCharacters(in: .whitespaces).isEmpty
                ) {
                    model.core.setGroup(session: session, group: name.trimmingCharacters(in: .whitespaces))
                    dialogs.dismiss()
                },
            ]
        ) {
            OutlinedField(placeholder: model.localize("manager-group-name"), text: $name)
        }
    }
}

private struct DeleteDialog: View {
    @Environment(AppModel.self) private var model
    @Environment(DialogPresenter.self) private var dialogs
    let session: Session

    var body: some View {
        DialogView(
            title: model.localize("manager-delete-title", ["name": session.name]),
            message: model.localize("manager-delete-message"),
            buttons: [
                DialogButton(label: model.localize("button-cancel")) { dialogs.dismiss() },
                DialogButton(label: model.localize("manager-delete-confirm"), kind: .destructive) {
                    model.core.deleteSession(session: session.id)
                    dialogs.dismiss()
                },
            ]
        )
    }
}
