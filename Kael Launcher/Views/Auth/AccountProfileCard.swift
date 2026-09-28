//
//  AccountProfileCard.swift
//  Kael Launcher
//

import SwiftUI

struct AccountProfileCard: View {
    @EnvironmentObject private var accountManager: AccountManager
    @Binding var isPresented: Bool
    @State private var faceImage: NSImage?

    private let avatarSize: CGFloat = 132

    var body: some View {
        VStack(spacing: 0) {
            banner
            rowsSection
                .padding(24)
            Divider()
            closeBar
        }
        .frame(width: 560)
        .background(Color(nsColor: .windowBackgroundColor))
        .clipShape(RoundedRectangle(cornerRadius: 24))
        .overlay(RoundedRectangle(cornerRadius: 24).strokeBorder(Color.secondary.opacity(0.2), lineWidth: 1))
        .shadow(radius: 30)
        .task(id: accountManager.activeSkin?.resolvedTextureKey) {
            await updateFaceImage()
        }
    }

    private var banner: some View {
        ZStack {
            RadialGradient(
                gradient: Gradient(colors: [Color.accentColor, Color(nsColor: .windowBackgroundColor)]),
                center: .top,
                startRadius: 10,
                endRadius: 340
            )

            VStack(spacing: 16) {
                Group {
                    if let faceImage {
                        Image(nsImage: faceImage)
                            .resizable()
                            .interpolation(.none)
                    } else {
                        RoundedRectangle(cornerRadius: 22)
                            .fill(Color.secondary.opacity(0.3))
                    }
                }
                .frame(width: avatarSize, height: avatarSize)
                .clipShape(RoundedRectangle(cornerRadius: 22))

                if let name = accountManager.activeAccount?.profile.name {
                    Text(name)
                        .font(.system(size: 32, weight: .bold))
                        .foregroundStyle(.white)
                }
            }
            .padding(.vertical, 40)
        }
        .frame(height: 280)
    }

    private var rowsSection: some View {
        VStack(spacing: 0) {
            ForEach(Array(accountManager.accounts.enumerated()), id: \.element.id) { index, account in
                AccountRow(account: account, isPresented: $isPresented)
                if index < accountManager.accounts.count - 1 {
                    Divider().padding(.leading, 70)
                }
            }
            if !accountManager.accounts.isEmpty {
                Divider().padding(.leading, 70)
            }
            AddAccountRow(isPresented: $isPresented)
        }
        .background(Color.secondary.opacity(0.12))
        .clipShape(RoundedRectangle(cornerRadius: 18))
    }

    private var closeBar: some View {
        HStack {
            Spacer()
            Button {
                isPresented = false
            } label: {
                Image(systemName: "checkmark")
                    .font(.system(size: 15, weight: .semibold))
                    .padding(.horizontal, 20)
                    .padding(.vertical, 8)
            }
            .buttonStyle(.plain)
            .background(Color.secondary.opacity(0.2))
            .clipShape(Capsule())
        }
        .padding(16)
    }

    private func updateFaceImage() async {
        guard let skin = accountManager.activeSkin else {
            faceImage = nil
            return
        }
        faceImage = try? await PlayerFaceRenderer.shared.face(for: skin, size: avatarSize * 2)
    }
}

private struct AccountRow: View {
    @EnvironmentObject private var accountManager: AccountManager
    let account: MinecraftCredentials
    @Binding var isPresented: Bool
    @State private var faceImage: NSImage?

    var body: some View {
        HStack(spacing: 12) {
            Button {
                isPresented = false
                Task { await accountManager.switchAccount(to: account.id) }
            } label: {
                HStack(spacing: 14) {
                    Group {
                        if let faceImage {
                            Image(nsImage: faceImage)
                                .resizable()
                                .interpolation(.none)
                        } else {
                            RoundedRectangle(cornerRadius: 8).fill(Color.secondary.opacity(0.2))
                        }
                    }
                    .frame(width: 36, height: 36)
                    .clipShape(RoundedRectangle(cornerRadius: 8))

                    Text(account.profile.name)
                        .font(.system(size: 16, weight: .semibold))
                        .foregroundStyle(
                            account.id == accountManager.activeAccount?.id ? Color.accentColor : Color.primary
                        )

                    Spacer()
                }
            }
            .buttonStyle(.plain)

            Button {
                Task { await accountManager.signOut(account.id) }
            } label: {
                Image(systemName: "trash")
                    .foregroundStyle(.secondary)
            }
            .buttonStyle(.plain)
        }
        .padding(.horizontal, 20)
        .padding(.vertical, 14)
        .task(id: account.currentSkin?.resolvedTextureKey) {
            guard let skin = account.currentSkin else {
                faceImage = nil
                return
            }
            faceImage = try? await PlayerFaceRenderer.shared.face(for: skin, size: 72)
        }
    }
}

private struct AddAccountRow: View {
    @EnvironmentObject private var accountManager: AccountManager
    @Binding var isPresented: Bool

    var body: some View {
        Button {
            isPresented = false
            Task { await accountManager.login() }
        } label: {
            HStack(spacing: 14) {
                Image(systemName: "person.badge.plus")
                    .font(.system(size: 18))
                    .frame(width: 36, height: 36)
                Text("Add Account")
                    .font(.system(size: 16, weight: .semibold))
                Spacer()
                Image(systemName: "chevron.right")
                    .foregroundStyle(.secondary)
            }
        }
        .buttonStyle(.plain)
        .padding(.horizontal, 20)
        .padding(.vertical, 14)
    }
}
