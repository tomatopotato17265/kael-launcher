//
//  ContentView.swift
//  Kael Launcher
//

import SwiftUI

struct ContentView: View {
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
        .animation(.easeInOut(duration: 0.15), value: isProfileCardPresented)
    }
}

#Preview {
    ContentView()
}
