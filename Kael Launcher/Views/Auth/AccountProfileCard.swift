//
//  AccountProfileCard.swift
//  Kael Launcher
//

import SwiftUI

struct AccountProfileCard: View {
    @EnvironmentObject private var accountManager: AccountManager
    @Binding var isPresented: Bool
    @State private var faceImage: NSImage?

    private let avatarSize: CGFloat = 96

    var body: some View {
        VStack(spacing: 0) {
            banner
            rowsSection
                .padding(20)
            Divider()
            closeBar
        }
        .frame(width: 420)
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
                endRadius: 260
            )

            VStack(spacing: 12) {
                Group {
                    if let faceImage {
                        Image(nsImage: faceImage)
                            .resizable()
                            .interpolation(.none)
                    } else {
                        RoundedRectangle(cornerRadius: 16)
                            .fill(Color.secondary.opacity(0.3))
                    }
                }
                .frame(width: avatarSize, height: avatarSize)
                .clipShape(RoundedRectangle(cornerRadius: 16))

                if let name = accountManager.activeAccount?.profile.name {
                    Text(name)
                        .font(.title2)
                        .fontWeight(.bold)
                        .foregroundStyle(.white)
                }
            }
            .padding(.vertical, 32)
        }
        .frame(height: 210)
    }

    private var rowsSection: some View {
        VStack(spacing: 0) {
            ForEach(Array(accountManager.accounts.enumerated()), id: \.element.id) { index, account in
                AccountRow(account: account, isPresented: $isPresented)
                if index < accountManager.accounts.count - 1 {
                    Divider().padding(.leading, 56)
                }
            }
            if !accountManager.accounts.isEmpty {
                Divider().padding(.leading, 56)
            }
            AddAccountRow(isPresented: $isPresented)
        }
        .background(Color.secondary.opacity(0.12))
        .clipShape(RoundedRectangle(cornerRadius: 16))
    }

    private var closeBar: some View {
        HStack {
            Spacer()
            Button {
                isPresented = false
            } label: {
                Image(systemName: "checkmark")
                    .fontWeight(.semibold)
                    .padding(.horizontal, 16)
                    .padding(.vertical, 6)
            }
            .buttonStyle(.plain)
            .background(Color.secondary.opacity(0.2))
            .clipShape(Capsule())
        }
        .padding(12)
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
                HStack(spacing: 12) {
                    Group {
                        if let faceImage {
                            Image(nsImage: faceImage)
                                .resizable()
                                .interpolation(.none)
                        } else {
                            RoundedRectangle(cornerRadius: 6).fill(Color.secondary.opacity(0.2))
                        }
                    }
                    .frame(width: 28, height: 28)
                    .clipShape(RoundedRectangle(cornerRadius: 6))

                    Text(account.profile.name)
                        .fontWeight(.semibold)
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
        .padding(.horizontal, 16)
        .padding(.vertical, 10)
        .task(id: account.currentSkin?.resolvedTextureKey) {
            guard let skin = account.currentSkin else {
                faceImage = nil
                return
            }
            faceImage = try? await PlayerFaceRenderer.shared.face(for: skin, size: 56)
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
            HStack(spacing: 12) {
                Image(systemName: "person.badge.plus")
                    .frame(width: 28, height: 28)
                Text("Add Account")
                    .fontWeight(.semibold)
                Spacer()
                Image(systemName: "chevron.right")
                    .foregroundStyle(.secondary)
            }
        }
        .buttonStyle(.plain)
        .padding(.horizontal, 16)
        .padding(.vertical, 10)
    }
}
