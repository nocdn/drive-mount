import SwiftUI

struct ContentView: View {
    @Environment(ConnectionListViewModel.self) private var viewModel
    @State private var isAddingConnection = false

    var body: some View {
        NavigationStack {
            List {
                if !viewModel.b2Connections.isEmpty {
                    b2Section
                }

                if viewModel.connections.isEmpty {
                    Section {
                        ContentUnavailableView(
                            "No Connections",
                            systemImage: "externaldrive.badge.plus",
                            description: Text("Add a connection to make it available in Files.")
                        )
                    }
                } else if !viewModel.otherConnections.isEmpty {
                    Section {
                        ForEach(viewModel.otherConnections) { connection in
                            NavigationLink(value: connection.id) {
                                ConnectionRow(connection: connection)
                            }
                        }
                        .onDelete { offsets in
                            Task {
                                await viewModel.deleteOtherConnections(at: offsets)
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

    @ViewBuilder
    private var b2Section: some View {
        let rows = viewModel.b2BucketRows
        let firstB2ID = viewModel.b2Connections.first?.id
        let status = viewModel.b2Connections.first.map(b2StatusCaption) ?? ""

        Section("Backblaze B2") {
            if rows.isEmpty {
                Text("Add a bucket name to show it in Files. Credentials are shared.")
                    .foregroundStyle(.secondary)
            } else {
                ForEach(rows) { row in
                    NavigationLink(value: row.connectionID) {
                        Label {
                            VStack(alignment: .leading) {
                                Text(row.name)
                                Text(status)
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                            }
                        } icon: {
                            Image(systemName: CloudProvider.backblazeB2.symbolName)
                        }
                    }
                    .accessibilityIdentifier("b2-bucket-row-\(row.name)")
                }
                .onDelete { offsets in
                    Task {
                        await viewModel.deleteB2Buckets(at: offsets)
                    }
                }
            }

            if let firstB2ID {
                NavigationLink(value: firstB2ID) {
                    Label("Credentials & buckets", systemImage: "key")
                }
                .accessibilityIdentifier("connection-row-b2")
            }
        }
    }

    private func b2StatusCaption(for connection: CloudConnection) -> String {
        let configured = connection.hasMinimumConfiguration ? "Shared credentials" : "Needs credentials"
        let enabled = connection.isEnabled ? "Enabled" : "Disabled"
        return "\(configured) - \(enabled)"
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
