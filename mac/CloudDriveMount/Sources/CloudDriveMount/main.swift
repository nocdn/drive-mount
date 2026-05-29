import AppKit

if CommandLine.arguments.contains("--clean-restart") {
    RuntimeLog.clear()
}

NSSetUncaughtExceptionHandler { exception in
    RuntimeLog.error("Uncaught exception: \(exception.name.rawValue) reason=\(exception.reason ?? "none") stack=\(exception.callStackSymbols.joined(separator: " | "))")
}

RuntimeLog.info("Process starting. pid=\(ProcessInfo.processInfo.processIdentifier) log=\(RuntimeLog.logFile.path)")

let app = NSApplication.shared
let appDelegate = AppDelegate()

app.delegate = appDelegate
app.setActivationPolicy(.accessory)
app.run()
