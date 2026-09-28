//
//  AccountButtonView.swift
//  Kael Launcher
//

import SwiftUI

struct AccountButtonView: View {
    @EnvironmentObject private var accountManager: AccountManager
    @Binding var isProfileCardPresented: Bool
    @State private var faceImage: NSImage?

    private let buttonSize: CGFloat = 28

    var body: some View {
        Button {
            if accountManager.activeAccount == nil {
                Task { await accountManager.login() }
            } else {
                isProfileCardPresented.toggle()
            }
        } label: {
            Group {
                if let faceImage {
                    Image(nsImage: faceImage)
                        .resizable()
                        .interpolation(.none)
                } else {
                    RoundedRectangle(cornerRadius: 6)
                        .fill(Color.secondary.opacity(0.2))
                }
            }
            .frame(width: buttonSize, height: buttonSize)
            .clipShape(RoundedRectangle(cornerRadius: 6))
            .overlay(RoundedRectangle(cornerRadius: 6).strokeBorder(Color.secondary.opacity(0.3), lineWidth: 1))
            .overlay {
                if accountManager.isSigningIn {
                    RoundedRectangle(cornerRadius: 6)
                        .fill(.black.opacity(0.4))
                    ProgressView()
                        .controlSize(.small)
                        .tint(.white)
                }
            }
        }
        .buttonStyle(.plain)
        .disabled(accountManager.isSigningIn)
        .task(id: accountManager.activeSkin?.resolvedTextureKey) {
            await updateFaceImage()
        }
    }

    private func updateFaceImage() async {
        guard let skin = accountManager.activeSkin else {
            faceImage = nil
            return
        }
        faceImage = try? await PlayerFaceRenderer.shared.face(for: skin, size: buttonSize * 2)
    }
}
