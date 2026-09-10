import AppKit

final class AppDelegate: NSObject, NSApplicationDelegate {
    private var browser: BrowserWindowController?

    func applicationDidFinishLaunching(_ notification: Notification) {
        do {
            let client = try CPKClient()
            let browser = BrowserWindowController(client: client)
            self.browser = browser
            browser.installMainMenu()
            browser.show()
        } catch {
            let alert = NSAlert()
            alert.messageText = "Cyber-Pumpkin could not start"
            alert.informativeText = error.localizedDescription
            alert.runModal()
            NSApp.terminate(nil)
        }
    }

    func applicationShouldTerminateAfterLastWindowClosed(
        _ sender: NSApplication
    ) -> Bool {
        true
    }
}

let application = NSApplication.shared
let delegate = AppDelegate()
application.delegate = delegate
application.setActivationPolicy(.regular)
application.run()
