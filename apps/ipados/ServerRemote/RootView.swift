import SwiftUI
import UniformTypeIdentifiers

/// The sidebar of pages and open connections, beside the page on show.
struct RootView: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        NavigationSplitView {
            Sidebar()
        } detail: {
            NavigationStack {
                PageView(page: model.state.page)
            }
        }
        .appDialog()
        .shellRequests()
    }
}

private struct Sidebar: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        let selection = Binding<Page?>(
            get: { model.state.page },
            set: { page in
                if let page { model.core.showPage(page: page) }
            }
        )
        List(selection: selection) {
            Label(model.localize("tab-manager"), systemImage: "square.grid.2x2")
                .tag(Page.manager)
            Label(model.localize("tab-connect"), systemImage: "plus")
                .tag(Page.connect)
            if !model.state.connections.isEmpty {
                Section {
                    ForEach(model.state.connections, id: \.id) { connection in
                        ConnectionRow(connection: connection)
                            .tag(Page.connection(id: connection.id))
                    }
                }
            }
        }
        .navigationTitle("ServerRemote")
    }
}

private struct ConnectionRow: View {
    @Environment(AppModel.self) private var model
    let connection: ConnectionState

    var body: some View {
        Label {
            Text(connection.name)
        } icon: {
            PhaseDot(phase: connection.phase)
        }
        .swipeActions {
            Button(model.localize("close-tab"), role: .destructive) {
                model.core.closeConnection(connection: connection.id)
            }
        }
        .contextMenu {
            Button(model.localize("close-tab"), systemImage: "xmark", role: .destructive) {
                model.core.closeConnection(connection: connection.id)
            }
        }
    }
}

struct PhaseDot: View {
    let phase: ConnectionPhase

    var body: some View {
        Circle()
            .fill(color)
            .frame(width: 10, height: 10)
    }

    private var color: Color {
        switch phase {
        case .connecting: .yellow
        case .connected: .green
        case .disconnected: .red
        }
    }
}

private struct PageView: View {
    @Environment(AppModel.self) private var model
    let page: Page

    var body: some View {
        switch page {
        case .manager:
            ManagerView()
        case .connect, .settings:
            ConnectView()
        case let .connection(id):
            if let connection = model.connection(id) {
                ConnectionView(connection: connection)
            } else {
                ContentUnavailableView(model.localize("connection-closed"), systemImage: "xmark")
            }
        }
    }
}

// MARK: - App dialogs

private extension View {
    func appDialog() -> some View {
        modifier(AppDialogModifier())
    }

    func shellRequests() -> some View {
        modifier(ShellRequestsModifier())
    }
}

private struct AppDialogModifier: ViewModifier {
    @Environment(AppModel.self) private var model

    func body(content: Content) -> some View {
        let dialog = model.state.dialog
        let shown = Binding(
            get: { dialog != nil },
            set: { if !$0 { model.core.closeDialog() } }
        )
        content.alert(title(dialog), isPresented: shown, presenting: dialog) { dialog in
            if case let .connectionFailed(_, details) = dialog {
                Button(model.localize("copy-details")) {
                    UIPasteboard.general.string = details
                    model.core.closeDialog()
                }
            }
            Button(model.localize("button-close"), role: .cancel) {
                model.core.closeDialog()
            }
        } message: { dialog in
            switch dialog {
            case let .error(_, message), let .connectionFailed(message, _):
                Text(message)
            }
        }
    }

    private func title(_ dialog: AppDialog?) -> String {
        switch dialog {
        case let .error(title, _): title
        case .connectionFailed: model.localize("connection-failed-title")
        case nil: ""
        }
    }
}

/// File pickers and savers the core asks for.
private struct ShellRequestsModifier: ViewModifier {
    @Environment(AppModel.self) private var model

    func body(content: Content) -> some View {
        content
            .fileImporter(
                isPresented: isRequested(\.opensFile),
                allowedContentTypes: importTypes,
                allowsMultipleSelection: false,
                onCompletion: { result in
                    let request = model.request
                    model.request = nil
                    opened(result.map { $0.first }, for: request)
                },
                onCancellation: { model.request = nil }
            )
            .fileExporter(
                isPresented: isRequested(\.savesFile, clearedOnDismiss: true),
                document: exportDocument,
                contentType: tomlType,
                defaultFilename: exportFileName,
                onCompletion: { result in
                    switch result {
                    case .success:
                        model.core.exported(error: nil)
                    case let .failure(error):
                        model.core.exported(error: error.localizedDescription)
                    }
                }
            )
    }

    private func opened(_ result: Result<URL?, Error>, for request: ShellRequest?) {
        switch request {
        case .importSessions:
            do {
                guard let url = try result.get() else { return }
                model.core.imported(contents: try readFile(url), error: nil)
            } catch {
                model.core.imported(contents: nil, error: error.localizedDescription)
            }
        case let .pickFile(connection, action, _):
            let url = try? result.get()
            // The connector reads the file for as long as it's in use (a
            // mounted disc image), so access isn't given back.
            _ = url?.startAccessingSecurityScopedResource()
            model.core.filePicked(connection: connection, action: action, path: url?.path)
        case .exportSession, nil:
            break
        }
    }

    /// Whether the pending request is of `kind`. The importer clears it in
    /// its handlers, which need to read it first; the exporter, which has no
    /// cancellation handler, when it's dismissed.
    private func isRequested(
        _ kind: KeyPath<ShellRequest, Bool>, clearedOnDismiss: Bool = false
    ) -> Binding<Bool> {
        Binding(
            get: { model.request?[keyPath: kind] ?? false },
            set: { shown in
                if !shown && clearedOnDismiss { model.request = nil }
            }
        )
    }

    private var tomlType: UTType {
        UTType(filenameExtension: "toml") ?? .plainText
    }

    private var importTypes: [UTType] {
        switch model.request {
        case let .pickFile(_, _, extensions) where !extensions.isEmpty:
            extensions.compactMap { UTType(filenameExtension: $0) }
        case .pickFile:
            [.item]
        default:
            [tomlType]
        }
    }

    private var exportDocument: TextDocument? {
        guard case let .exportSession(_, contents) = model.request else { return nil }
        return TextDocument(text: contents)
    }

    private var exportFileName: String? {
        guard case let .exportSession(fileName, _) = model.request else { return nil }
        return fileName
    }

    private func readFile(_ url: URL) throws -> String {
        let accessing = url.startAccessingSecurityScopedResource()
        defer { if accessing { url.stopAccessingSecurityScopedResource() } }
        return try String(contentsOf: url, encoding: .utf8)
    }
}

private extension ShellRequest {
    var opensFile: Bool {
        switch self {
        case .importSessions, .pickFile: true
        case .exportSession: false
        }
    }

    var savesFile: Bool {
        if case .exportSession = self { true } else { false }
    }
}

/// An exported session, for the file exporter.
struct TextDocument: FileDocument {
    static let readableContentTypes: [UTType] = [.plainText]
    let text: String

    init(text: String) {
        self.text = text
    }

    init(configuration: ReadConfiguration) throws {
        text = String(decoding: configuration.file.regularFileContents ?? Data(), as: UTF8.self)
    }

    func fileWrapper(configuration: WriteConfiguration) throws -> FileWrapper {
        FileWrapper(regularFileWithContents: Data(text.utf8))
    }
}
