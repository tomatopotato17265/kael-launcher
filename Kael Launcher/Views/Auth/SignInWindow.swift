//
//  SignInWindow.swift
//  Kael Launcher
//

import AppKit
import Foundation
import WebKit

@MainActor
final class SignInWindowController: NSObject, WKNavigationDelegate, NSWindowDelegate {
    private static var activeControllers: [ObjectIdentifier: SignInWindowController] = [:]

    private var window: NSWindow?
    private var continuation: CheckedContinuation<String?, Never>?
    private var isFinished = false

    static func presentSignIn(url: URL) async -> String? {
        let controller = SignInWindowController()
        activeControllers[ObjectIdentifier(controller)] = controller

        return await withCheckedContinuation { continuation in
            controller.continuation = continuation
            controller.present(url: url)
        }
    }

    private func present(url: URL) {
        let webView = WKWebView(frame: .zero)
        webView.navigationDelegate = self

        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 1000, height: 700),
            styleMask: [.titled, .closable, .resizable, .miniaturizable],
            backing: .buffered,
            defer: false
        )
        window.title = "Sign in to Minecraft"
        window.minSize = NSSize(width: 500, height: 500)
        window.setContentSize(NSSize(width: 1000, height: 700))
        window.center()
        window.contentView = webView
        window.delegate = self
        window.isReleasedWhenClosed = false
        window.level = .floating
        self.window = window

        window.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
        NSApp.requestUserAttention(.criticalRequest)

        webView.load(URLRequest(url: url))
    }

    func webView(
        _ webView: WKWebView,
        decidePolicyFor navigationAction: WKNavigationAction,
        decisionHandler: @escaping (WKNavigationActionPolicy) -> Void
    ) {
        if let url = navigationAction.request.url,
           url.absoluteString.hasPrefix("https://login.live.com/oauth20_desktop.srf"),
           let components = URLComponents(url: url, resolvingAgainstBaseURL: false),
           let code = components.queryItems?.first(where: { $0.name == "code" })?.value {
            decisionHandler(.cancel)
            finish(code: code)
            return
        }
        decisionHandler(.allow)
    }

    func windowWillClose(_ notification: Notification) {
        finish(code: nil)
    }

    private func finish(code: String?) {
        guard !isFinished else {
            return
        }
        isFinished = true
        Self.activeControllers.removeValue(forKey: ObjectIdentifier(self))
        continuation?.resume(returning: code)
        continuation = nil
        window?.close()
    }
}
