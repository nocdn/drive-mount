import SwiftUI

struct AddConnectionView: View {
    @Environment(ConnectionListViewModel.self) private var viewModel
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            List(CloudProvider.allCases) { provider in
                Button {
                    Task {
                        await viewModel.addConnection(provider: provider)
                        dismiss()
                    }
                } label: {
                    Label(provider.displayName, systemImage: provider.symbolName)
                }
                .accessibilityIdentifier("add-provider-\(provider.rawValue)")
            }
            .navigationTitle("Add Connection")
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") {
                        dismiss()
                    }
                }
            }
        }
    }
}

#Preview {
    AddConnectionView()
        .environment(ConnectionListViewModel.preview)
}
