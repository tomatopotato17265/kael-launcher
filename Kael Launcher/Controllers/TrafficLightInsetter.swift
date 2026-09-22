//
//  TrafficLightInsetter.swift
//  Kael Launcher
//

import SwiftUI
import AppKit

final class TrafficLightPositioningView: NSView {
    var offset: CGPoint = .zero {
        didSet { applyOffset() }
    }

    private var originalPositions: [NSWindow.ButtonType: CGPoint] = [:]
    private var isObservingWindow = false

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        capturePositionsIfNeeded()
        applyOffset()
        observeWindowIfNeeded()

        for delay in [0.0, 0.05, 0.2] {
            DispatchQueue.main.asyncAfter(deadline: .now() + delay) { [weak self] in
                self?.capturePositionsIfNeeded()
                self?.applyOffset()
            }
        }
    }

    override func layout() {
        super.layout()
        capturePositionsIfNeeded()
        applyOffset()
    }

    private func observeWindowIfNeeded() {
        guard let window, !isObservingWindow else { return }
        isObservingWindow = true
        NotificationCenter.default.addObserver(
            self, selector: #selector(windowDidLayout),
            name: NSWindow.didResizeNotification, object: window
        )
        NotificationCenter.default.addObserver(
            self, selector: #selector(windowDidLayout),
            name: NSWindow.didBecomeKeyNotification, object: window
        )
    }

    @objc private func windowDidLayout() {
        applyOffset()
    }

    private func capturePositionsIfNeeded() {
        guard let window, originalPositions.isEmpty else { return }
        for type: NSWindow.ButtonType in [.closeButton, .miniaturizeButton, .zoomButton] {
            if let button = window.standardWindowButton(type) {
                originalPositions[type] = button.frame.origin
            }
        }
    }

    private func applyOffset() {
        guard let window else { return }
        for (type, origin) in originalPositions {
            window.standardWindowButton(type)?.setFrameOrigin(
                CGPoint(x: origin.x + offset.x, y: origin.y - offset.y)
            )
        }
    }
}

struct TrafficLightInsetter: NSViewRepresentable {
    var offset: CGPoint

    func makeNSView(context: Context) -> TrafficLightPositioningView {
        let view = TrafficLightPositioningView()
        view.offset = offset
        return view
    }

    func updateNSView(_ nsView: TrafficLightPositioningView, context: Context) {
        nsView.offset = offset
    }
}
