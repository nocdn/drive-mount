import SwiftUI

struct ConnectionEditorView: View {
    @Environment(ConnectionListViewModel.self) private var viewModel
    @Environment(\.dismiss) private var dismiss
    @Binding var connection: CloudConnection

    var body: some View {
        Form {
            Section("Connection") {
                if connection.provider != .backblazeB2 {
                    TextField("Name", text: $connection.displayName)
                        .textContentType(.name)
                        .accessibilityIdentifier("connection-name-field")
                }
                Toggle("Show in Files", isOn: $connection.isEnabled)
                LabeledContent("Type", value: connection.provider.displayName)
            }

            switch connection.provider {
            case .backblazeB2:
                B2Fields(settings: $connection.b2)
            case .googleDrive:
                GoogleDriveFields(settings: $connection.googleDrive)
            case .oneDrive:
                OneDriveFields(settings: $connection.oneDrive)
            case .seedbox:
                SeedboxFields(settings: $connection.seedbox)
            }

            Section {
                Button {
                    Task {
                        await viewModel.saveConnection(connection)
                    }
                } label: {
                    Label("Save", systemImage: "checkmark")
                }
                .accessibilityIdentifier("save-connection-button")
            }

            if !connection.hasMinimumConfiguration {
                Section {
                    Text("Files can show this connection after the required credentials are saved.")
                        .foregroundStyle(.secondary)
                }
            }

            if connection.provider == .backblazeB2 {
                Section {
                    Button("Remove Backblaze B2", role: .destructive) {
                        Task {
                            await viewModel.deleteConnection(id: connection.id)
                            dismiss()
                        }
                    }
                }
            }
        }
        .navigationTitle(connection.provider == .backblazeB2 ? "Backblaze B2" : connection.effectiveDisplayName)
        .navigationBarTitleDisplayMode(.inline)
    }
}

private struct B2Fields: View {
    @Binding var settings: B2ConnectionSettings

    var body: some View {
        Section("Credentials") {
            TextField("Key ID", text: $settings.applicationKeyID)
                .textInputAutocapitalization(.never)
                .textContentType(.username)
                .accessibilityIdentifier("b2-key-id-field")
            SecureField("Application Key", text: $settings.applicationKey)
                .textContentType(.password)
                .accessibilityIdentifier("b2-application-key-field")
            Text("These credentials are used for every bucket below.")
                .font(.caption)
                .foregroundStyle(.secondary)
        }

        Section("Buckets") {
            ForEach(settings.bucketNames.indices, id: \.self) { index in
                TextField("Bucket name", text: $settings.bucketNames[index])
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                    .accessibilityIdentifier(index == 0 ? "b2-bucket-field" : "b2-bucket-field-\(index)")
            }
            .onDelete { offsets in
                settings.bucketNames.remove(atOffsets: offsets)
                if settings.bucketNames.isEmpty {
                    settings.bucketNames = [""]
                }
            }

            Button("Add Bucket") {
                settings.bucketNames.append("")
            }
            .accessibilityIdentifier("b2-add-bucket-button")
        }
    }
}

private struct GoogleDriveFields: View {
    @Binding var settings: GoogleDriveConnectionSettings

    var body: some View {
        Section("Google Drive") {
            SecureField("OAuth access token", text: $settings.accessToken)
                .textContentType(.password)
            TextField("Root folder ID", text: $settings.rootFolderID)
                .textInputAutocapitalization(.never)
        }
    }
}

private struct OneDriveFields: View {
    @Binding var settings: OneDriveConnectionSettings

    var body: some View {
        Section("OneDrive") {
            SecureField("Microsoft Graph access token", text: $settings.accessToken)
                .textContentType(.password)
            TextField("Root item ID", text: $settings.rootItemID)
                .textInputAutocapitalization(.never)
        }
    }
}

private struct SeedboxFields: View {
    @Binding var settings: SeedboxConnectionSettings

    var body: some View {
        Section("Seedbox") {
            TextField("Host", text: $settings.host)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
                .textContentType(.URL)
                .accessibilityIdentifier("seedbox-host-field")
            TextField("Username", text: $settings.username)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
                .textContentType(.username)
                .accessibilityIdentifier("seedbox-username-field")
            SecureField("Password", text: $settings.password)
                .textContentType(.password)
                .accessibilityIdentifier("seedbox-password-field")
            Stepper(value: $settings.port, in: 1...65535) {
                LabeledContent("SFTP Port", value: "\(settings.port)")
            }
            Text("SFTP is used for large downloads. Port 22 is the usual seedbox port.")
                .font(.caption)
                .foregroundStyle(.secondary)
            if settings.port == 21 {
                Text("Port 21 is the old FTPS default. Files will connect with SFTP on port 22.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            TextField("Remote path", text: $settings.remotePath)
                .textInputAutocapitalization(.never)
            Toggle("Read Only", isOn: $settings.readOnly)
        }
    }
}

#Preview {
    @Previewable @State var connection = CloudConnection(provider: .backblazeB2)
    NavigationStack {
        ConnectionEditorView(connection: $connection)
            .environment(ConnectionListViewModel.preview)
    }
}
