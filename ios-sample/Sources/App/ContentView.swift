import SwiftUI
import TelemetryParser

struct ContentView: View {
    @State private var status = "Pick a video file to parse telemetry."
    @State private var isParsing = false

    var body: some View {
        NavigationStack {
            VStack(alignment: .leading, spacing: 16) {
                Text(status)
                    .font(.body)
                    .frame(maxWidth: .infinity, alignment: .leading)

                Button("Select video") {
                    // Wire up UIDocumentPickerViewController or PHPicker in your app.
                    // Copy the selected file to FileManager.default.temporaryDirectory,
                    // then call parseVideo(at: tempURL.path).
                    status = "Integrate a document picker and call parseVideo(at:)."
                }
                .disabled(isParsing)
            }
            .padding()
            .navigationTitle("Telemetry Sample")
        }
    }

    private func parseVideo(at path: String) {
        isParsing = true
        status = "Parsing…"
        DispatchQueue.global(qos: .userInitiated).async {
            do {
                try withTelemetryParser(path: path) { parser in
                    let summary = """
                    Camera: \(try parser.getCameraType())
                    Duration: \(try parser.getDuration())s
                    GPS: \(try parser.getGpsData().count)
                    Accel: \(try parser.getAccelerometerData().count)
                    Gyro: \(try parser.getGyroscopeData().count)
                    """
                    DispatchQueue.main.async {
                        status = summary
                        isParsing = false
                    }
                }
            } catch {
                DispatchQueue.main.async {
                    status = error.localizedDescription
                    isParsing = false
                }
            }
        }
    }
}

#Preview {
    ContentView()
}
