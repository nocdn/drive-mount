import SwiftUI

@main
struct DriveMountApp: App {
    @State private var viewModel = ConnectionListViewModel()

    var body: some Scene {
        WindowGroup {
            ContentView()
                .environment(viewModel)
                .task {
                    await viewModel.bootstrap()
                }
        }
    }
}
