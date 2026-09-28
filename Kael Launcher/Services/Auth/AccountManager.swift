//
//  AccountManager.swift
//  Kael Launcher
//

import Combine
import Foundation

@MainActor
final class AccountManager: ObservableObject {
    static let shared = AccountManager()

    @Published private(set) var accounts: [MinecraftCredentials] = []
    @Published private(set) var activeAccount: MinecraftCredentials?
    @Published private(set) var isSigningIn = false
    @Published var lastError: Error?

    private let authService = MinecraftAuthService.shared
    private let credentialStore = CredentialStore.shared
    private var errorToken = 0

    var activeSkin: MinecraftSkin? {
        activeAccount?.currentSkin
    }

    init() {
        Task {
            await refreshAccounts()
        }
    }

    func refreshAccounts() async {
        do {
            let stored = try await credentialStore.allCredentials()

            var refreshed: [MinecraftCredentials] = []
            for credentials in stored {
                do {
                    refreshed.append(try await authService.refresh(credentials))
                } catch MinecraftAuthError.oauthError("invalid_grant") {
                    try? await credentialStore.remove(id: credentials.id)
                } catch {
                    refreshed.append(credentials)
                }
            }
            refreshed.sort { $0.profile.name.localizedCaseInsensitiveCompare($1.profile.name) == .orderedAscending }

            accounts = refreshed

            let activeId = try await credentialStore.activeAccountId()
            if let match = refreshed.first(where: { $0.id == activeId }) {
                activeAccount = match
            } else if let fallback = refreshed.first {
                activeAccount = fallback
                try? await credentialStore.setActive(id: fallback.id)
            } else {
                activeAccount = nil
            }
        } catch {
            present(error)
        }
    }

    func login() async {
        guard !isSigningIn else {
            return
        }
        isSigningIn = true
        defer { isSigningIn = false }

        do {
            let flow = try await authService.beginLogin()
            guard let code = await SignInWindowController.presentSignIn(url: flow.authRequestURI) else {
                return
            }
            _ = try await authService.finishLogin(code: code, flow: flow)
            await refreshAccounts()
        } catch {
            present(error)
        }
    }

    func switchAccount(to id: UUID) async {
        do {
            try await credentialStore.setActive(id: id)
            await refreshAccounts()
        } catch {
            present(error)
        }
    }

    func signOut(_ id: UUID) async {
        do {
            try await credentialStore.remove(id: id)
            await refreshAccounts()

            if activeAccount == nil, let fallback = accounts.first {
                try await credentialStore.setActive(id: fallback.id)
                await refreshAccounts()
            }
        } catch {
            present(error)
        }
    }

    private func present(_ error: Error) {
        errorToken += 1
        let token = errorToken
        lastError = error

        Task {
            try? await Task.sleep(nanoseconds: 4_000_000_000)
            if errorToken == token {
                lastError = nil
            }
        }
    }
}
