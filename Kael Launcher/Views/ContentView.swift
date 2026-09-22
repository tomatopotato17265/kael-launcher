//
//  ContentView.swift
//  Kael Launcher
//

import SwiftUI

struct ContentView: View {
    @State private var selectedTab: AppTab = .home

    var body: some View {
        VStack {
            Image(systemName: "globe")
                .imageScale(.large)
                .foregroundStyle(.tint)
            Text("Hello, world!")
        }
        .padding()
        .toolbar {
            ToolbarItem(placement: .principal) {
                Picker("", selection: $selectedTab) {
                    ForEach(AppTab.allCases) { tab in
                        if tab == .search {
                            Image(systemName: "magnifyingglass").tag(tab)
                        } else {
                            Text(tab.rawValue).tag(tab)
                        }
                    }
                }
                .pickerStyle(.segmented)
                .frame(width: 360)
            }
        }
    }
}

#Preview {
    ContentView()
}
