import SwiftUI

struct ContentView: View {
    @Environment(ConnectionListViewModel.self) private var viewModel
    @State private var isAddingConnection = false

    var body: some View {
        NavigationStack {
            List {
                Section {
                    if viewModel.connections.isEmpty {
                        ContentUnavailableView(
                            "No Connections",
                            systemImage: "externaldrive.badge.plus",
                            description: Text("Add a connection to make it available in Files.")
                        )
                    } else {
                        ForEach(viewModel.connections) { connection in
                            NavigationLink(value: connection.id) {
                                ConnectionRow(connection: connection)
                            }
                        }
                        .onDelete { offsets in
                            Task {
                                await viewModel.deleteConnections(at: offsets)
                            }
                        }
                    }
                }

                Section("Files") {
                    HStack {
                        Label("Registered domains", systemImage: "folder")
                        Spacer()
                        Text("\(viewModel.registeredDomainCount)")
                            .foregroundStyle(.secondary)
                    }
                    Button {
                        Task {
                            await viewModel.syncFileProviderDomains()
                        }
                    } label: {
                        Label("Refresh Files Registration", systemImage: "arrow.clockwise")
                    }
                }

                if !viewModel.statusMessage.isEmpty {
                    Section {
                        Text(viewModel.statusMessage)
                            .foregroundStyle(.secondary)
                    }
                }
            }
            .navigationTitle("Drive Mount")
            .navigationDestination(for: String.self) { id in
                if let binding = viewModel.binding(for: id) {
                    ConnectionEditorView(connection: binding)
                        .environment(viewModel)
                } else {
                    ContentUnavailableView("Connection Missing", systemImage: "questionmark.folder")
                }
            }
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) {
                    Button {
                        isAddingConnection = true
                    } label: {
                        Label("Add Connection", systemImage: "plus")
                    }
                    .accessibilityIdentifier("add-connection-button")
                }
            }
            .sheet(isPresented: $isAddingConnection) {
                AddConnectionView()
                    .environment(viewModel)
            }
            .refreshable {
                await viewModel.syncFileProviderDomains()
            }
        }
    }
}

private struct ConnectionRow: View {
    var connection: CloudConnection

    var body: some View {
        Label {
            VStack(alignment: .leading) {
                Text(connection.effectiveDisplayName)
                Text(rowDetail)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        } icon: {
            Image(systemName: connection.provider.symbolName)
        }
        .accessibilityIdentifier("connection-row-\(connection.provider.rawValue)")
    }

    private var rowDetail: String {
        let configured = connection.hasMinimumConfiguration ? "Configured" : "Needs credentials"
        let enabled = connection.isEnabled ? "Enabled" : "Disabled"
        return "\(connection.provider.displayName) - \(configured) - \(enabled)"
    }
}

#Preview {
    ContentView()
        .environment(ConnectionListViewModel.preview)
}
