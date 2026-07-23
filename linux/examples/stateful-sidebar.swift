@State private var count = 0
@State private var enabled = true
@State private var name = "Linux"
@State private var intensity = 0.5
@State private var mode = "balanced"
@State private var retries = 2
@State private var modeTransition = ""
@State private var submittedName = ""

VStack(alignment: .leading, spacing: 8) {
    Text("Stateful sidebar").font(.headline)
    Text("Hello \(name)")
    Text("Count \(count)")
    Button("Increment") {
        count += 1
    }
    Toggle("Enabled", isOn: $enabled)
    TextField("Name", text: $name)
        .onSubmit {
            submittedName = name
        }
    Slider(value: $intensity, in: 0.0...1.0, step: 0.1) {
        Text("Intensity")
    }
    Picker("Mode", selection: $mode) {
        Text("Fast").tag("fast")
        Text("Balanced").tag("balanced")
        Text("Thorough").tag("thorough")
    }
    .onChange(of: mode) { oldValue, newValue in
        modeTransition = "\(oldValue) -> \(newValue)"
    }
    Stepper("Retries \(retries)", value: $retries, in: 0...10, step: 1)
    Text("Mode: \(modeTransition)")
    Text("Submitted: \(submittedName)")
}
