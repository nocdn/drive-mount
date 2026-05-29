import Darwin
import Foundation

final class SingleInstanceCoordinator: NSObject {
    private static let showSettingsNotification = Notification.Name("com.bartek.clouddrivemount.show-settings")

    private let lockURL: URL
    private var lockFileDescriptor: Int32 = -1
    private var showSettingsHandler: (@MainActor () -> Void)?

    override init() {
        lockURL = FileManager.default.homeDirectoryForCurrentUser
            .appendingPathComponent("Library")
            .appendingPathComponent("Application Support")
            .appendingPathComponent("CloudDriveMount")
            .appendingPathComponent("CloudDriveMount.lock")
        super.init()
    }

    func claimOrSignalRunningInstance() -> Bool {
        do {
            try FileManager.default.createDirectory(
                at: lockURL.deletingLastPathComponent(),
                withIntermediateDirectories: true
            )
        } catch {
            RuntimeLog.error("Could not create single-instance lock directory: \(error.localizedDescription)")
            return true
        }

        let fd = open(lockURL.path, O_CREAT | O_RDWR, S_IRUSR | S_IWUSR)
        guard fd != -1 else {
            RuntimeLog.error("Could not open single-instance lock file: errno \(errno)")
            return true
        }

        if flock(fd, LOCK_EX | LOCK_NB) == 0 {
            lockFileDescriptor = fd
            RuntimeLog.info("Acquired single-instance lock at \(lockURL.path)")
            return true
        }

        let lockError = errno
        close(fd)

        if lockError == EWOULDBLOCK || lockError == EAGAIN {
            RuntimeLog.info("Another Cloud Drive Mount instance is already running; requesting Settings window.")
            signalRunningInstance()
            return false
        }

        RuntimeLog.error("Could not acquire single-instance lock: errno \(lockError)")
        return true
    }

    func listenForShowSettingsRequests(_ handler: @escaping @MainActor () -> Void) {
        showSettingsHandler = handler
        DistributedNotificationCenter.default().addObserver(
            self,
            selector: #selector(handleShowSettingsRequest),
            name: Self.showSettingsNotification,
            object: nil
        )
    }

    func stop() {
        DistributedNotificationCenter.default().removeObserver(self)
        showSettingsHandler = nil
        releaseLock()
    }

    private func signalRunningInstance() {
        DistributedNotificationCenter.default().postNotificationName(
            Self.showSettingsNotification,
            object: nil,
            userInfo: nil,
            deliverImmediately: true
        )
    }

    @objc private func handleShowSettingsRequest(_ notification: Notification) {
        RuntimeLog.info("Received show-settings request from another instance")
        guard let showSettingsHandler else { return }
        Task { @MainActor in
            showSettingsHandler()
        }
    }

    private func releaseLock() {
        guard lockFileDescriptor != -1 else { return }
        flock(lockFileDescriptor, LOCK_UN)
        close(lockFileDescriptor)
        lockFileDescriptor = -1
    }

    deinit {
        DistributedNotificationCenter.default().removeObserver(self)
        releaseLock()
    }
}
