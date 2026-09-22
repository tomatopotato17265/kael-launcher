//
//  ContentView.swift
//  Kael Launcher
//

import SwiftUI

struct ContentView: View {
    @State private var selectedTab: AppTab = .home

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
    }
}

#Preview {
    ContentView()
}
