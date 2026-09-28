//
//  ContentView.swift
//  Kael Launcher
//

import SwiftUI

struct ContentView: View {
    @EnvironmentObject private var accountManager: AccountManager
    @State private var selectedTab: AppTab = .home
    @State private var isProfileCardPresented = false

    var body: some View {
        TabView(selection: $selectedTab) {
            ForEach(AppTab.allCases) { tab in
                Tab(value: tab) {
                    VStack {
                        Image(systemName: "globe")
                            .imageScale(.large)
                            .foregroundStyle(.tint)
                        Text("Hello, world!")
                    }
                    .padding()
                } label: {
                    Text(tab.rawValue)
                }
            }
        }
        .tabViewStyle(.tabBarOnly)
        .overlay {
            if isProfileCardPresented {
                Color.black.opacity(0.35)
                    .onTapGesture { isProfileCardPresented = false }
                    .overlay {
                        AccountProfileCard(isPresented: $isProfileCardPresented)
                    }
                    .transition(.opacity)
            }
        }
        .overlay(alignment: .topTrailing) {
            AccountButtonView(isProfileCardPresented: $isProfileCardPresented)
                .padding(.top, 12)
                .padding(.trailing, 16)
                .ignoresSafeArea(edges: .top)
        }
        .overlay(alignment: .bottom) {
            if let error = accountManager.lastError {
                Text(error.localizedDescription)
                    .font(.footnote)
                    .foregroundStyle(.white)
                    .padding(.horizontal, 14)
                    .padding(.vertical, 10)
                    .background(Color.red.opacity(0.85), in: RoundedRectangle(cornerRadius: 10))
                    .padding(.bottom, 24)
                    .onTapGesture { accountManager.lastError = nil }
                    .transition(.move(edge: .bottom).combined(with: .opacity))
            }
        }
        .animation(.easeInOut(duration: 0.15), value: isProfileCardPresented)
        .animation(.easeInOut(duration: 0.2), value: accountManager.lastError?.localizedDescription)
    }
}

#Preview {
    ContentView()
}
