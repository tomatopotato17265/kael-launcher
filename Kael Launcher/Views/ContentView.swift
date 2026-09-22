//
//  ContentView.swift
//  Kael Launcher
//

import SwiftUI

struct ContentView: View {
    var body: some View {
        VStack {
            Image(systemName: "globe")
                .imageScale(.large)
                .foregroundStyle(.tint)
            Text("Hello, world!")
        }
        .padding()
        .background(TrafficLightInsetter(offset: WindowConfiguration.trafficLightOffset).frame(width: 0, height: 0))
    }
}

#Preview {
    ContentView()
}
