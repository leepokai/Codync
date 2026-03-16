//
//  CodePulseLiveActivityWidget.swift
//  CodePulseLiveActivity
//
//  Created by 李博凱 on 2026/3/16.
//

import ActivityKit
import WidgetKit
import SwiftUI

struct CodingSessionAttributes: ActivityAttributes {
    public struct ContentState: Codable, Hashable {
        var activeMinutes: Int
        var currentLanguage: String
    }
    var projectName: String
}

struct CodePulseLiveActivityWidget: Widget {
    let kind: String = "CodePulseLiveActivity"

    var body: some WidgetConfiguration {
        ActivityConfiguration(for: CodingSessionAttributes.self) { context in
            // Lock screen / banner UI
            VStack {
                Text("Coding: \(context.attributes.projectName)")
                    .font(.headline)
                Text("\(context.state.activeMinutes) min - \(context.state.currentLanguage)")
                    .font(.subheadline)
            }
            .padding()
        } dynamicIsland: { context in
            DynamicIsland {
                DynamicIslandExpandedRegion(.leading) {
                    Text(context.state.currentLanguage)
                }
                DynamicIslandExpandedRegion(.trailing) {
                    Text("\(context.state.activeMinutes)m")
                }
                DynamicIslandExpandedRegion(.center) {
                    Text(context.attributes.projectName)
                }
            } compactLeading: {
                Text(context.state.currentLanguage)
            } compactTrailing: {
                Text("\(context.state.activeMinutes)m")
            } minimal: {
                Text("\(context.state.activeMinutes)")
            }
        }
    }
}
