import SwiftUI
import UniformTypeIdentifiers

/// The window: a title bar of tabs, as on the desktop, above the page on
/// show, with any dialog over both.
struct RootView: View {
    @Environment(AppModel.self) private var model
    @State private var dialogs = DialogPresenter()

    var body: some View {
        VStack(spacing: 0) {
            TitleBar()
            PageView(page: model.state.page)
                .padding([.horizontal, .bottom], Metrics.spacing)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
        .background(Color.base.ignoresSafeArea())
        .font(.ui)
        .foregroundStyle(Color.text)
        .tint(.accent)
        .overlay { dialogLayer }
        .environment(dialogs)
        .shellRequests()
    }

    @ViewBuilder
    private var dialogLayer: some View {
        if let content = dialogs.content {
            Backdrop { content }
        } else if let dialog = model.state.dialog {
            Backdrop { AppDialogView(dialog: dialog) }
        }
    }
}

private struct TitleBar: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        HStack(spacing: Metrics.spacing) {
            IconTab(page: .manager, icon: .virtualDesktop)
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: Metrics.spacing) {
                    ForEach(model.state.connections, id: \.id) { connection in
                        ConnectionTab(connection: connection)
                    }
                    IconTab(page: .connect, icon: .addLarge)
                }
            }
        }
        .padding(Metrics.spacing)
    }
}

private struct IconTab: View {
    @Environment(AppModel.self) private var model
    let page: Page
    let icon: AppIcon

    var body: some View {
        Button {
            model.core.showPage(page: page)
        } label: {
            IconView(icon).frame(width: Metrics.controlHeight, height: Metrics.controlHeight)
        }
        .buttonStyle(BorderedStyle(active: model.state.page == page))
    }
}

private struct ConnectionTab: View {
    @Environment(AppModel.self) private var model
    let connection: ConnectionState

    private var active: Bool {
        model.state.page == .connection(id: connection.id)
    }

    var body: some View {
        Button {
            model.core.showPage(page: .connection(id: connection.id))
        } label: {
            HStack(spacing: Metrics.spacing) {
                IconView(.connectionProtocol(connection.protocol))
                Text(connection.name)
                    .lineLimit(1)
                    .truncationMode(.tail)
                    .frame(maxWidth: .infinity, alignment: .leading)
                Button {
                    model.core.closeConnection(connection: connection.id)
                } label: {
                    IconView(.close, size: Metrics.iconSize * 0.9)
                        .frame(width: 18 * Metrics.scale, height: 18 * Metrics.scale)
                        .overlay(
                            RoundedRectangle(cornerRadius: 4 * Metrics.scale)
                                .strokeBorder(active ? Color.strongest : Color.strong)
                        )
                }
                .buttonStyle(.plain)
            }
            .padding(.horizontal, 5 * Metrics.scale)
            .frame(width: 150 * Metrics.scale)
        }
        .buttonStyle(BorderedStyle(active: active))
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
                Text(model.localize("connection-closed"))
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
    }
}

/// A dialog the app put up.
private struct AppDialogView: View {
    @Environment(AppModel.self) private var model
    let dialog: AppDialog

    var body: some View {
        switch dialog {
        case let .error(title, message):
            DialogView(title: title, message: message, buttons: [
                DialogButton(label: model.localize("button-ok"), kind: .primary) {
                    model.core.closeDialog()
                },
            ])
        case let .connectionFailed(message, details):
            DialogView(title: model.localize("connection-failed-title"), message: message, buttons: [
                DialogButton(label: model.localize("copy-details"), icon: .copyToClipboard) {
                    UIPasteboard.general.string = details
                },
                DialogButton(label: model.localize("button-close")) {
                    model.core.closeDialog()
                },
            ])
        }
    }
}

// MARK: - Shell requests

private extension View {
    func shellRequests() -> some View {
        modifier(ShellRequestsModifier())
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
