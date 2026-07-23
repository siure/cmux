enum PanelMode: String {
    case overview
    case details
}

struct ModeSummary: View {
    let mode: PanelMode

    var body: some View {
        switch mode {
        case .overview:
            Text("Workspace overview")
        case .details:
            Text("Detailed workspace status")
        }
    }
}

@State private var mode = PanelMode.overview

VStack(alignment: .leading, spacing: 8) {
    Text("Enum sidebar").font(.headline)
    ModeSummary(mode: mode)
    Picker("Mode", selection: $mode) {
        Text("Overview").tag(PanelMode.overview)
        Text("Details").tag(PanelMode.details)
    }
}
