package io.github.telemetryparser.sample

import android.app.Activity
import android.content.Intent
import android.net.Uri
import android.os.Bundle
import android.widget.TextView
import androidx.activity.result.contract.ActivityResultContracts
import androidx.appcompat.app.AppCompatActivity
import com.google.android.material.button.MaterialButton
import io.github.telemetryparser.TelemetryParser
import java.io.File
import java.io.FileOutputStream

/**
 * Minimal integration test for [TelemetryParser]: pick an MP4, copy to cache (native API needs a path),
 * run the parser, show counts and first samples.
 *
 * **ABI:** The `:android` library defaults to `arm64-v8a` only. Use a physical device or build with
 * `-PrustAbis=arm64-v8a,x86_64` from the repo root before `:sample-app:installDebug` so an x86_64 emulator
 * loads `libtelemetry_parser.so`. Otherwise you get [UnsatisfiedLinkError].
 */
class TelemetrySampleActivity : AppCompatActivity() {

    private lateinit var tvLog: TextView

    private val pickVideo = registerForActivityResult(
        ActivityResultContracts.StartActivityForResult()
    ) { result ->
        if (result.resultCode == Activity.RESULT_OK) {
            result.data?.data?.let { uri -> processUri(uri) }
        }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_main)
        tvLog = findViewById(R.id.tvLog)
        findViewById<MaterialButton>(R.id.btnPickVideo).setOnClickListener {
            pickVideo.launch(
                Intent(Intent.ACTION_OPEN_DOCUMENT).apply {
                    addCategory(Intent.CATEGORY_OPENABLE)
                    type = "video/*"
                }
            )
        }

        tvLog.text =
            """
            TelemetryParser sample — pick a GoPro/DJI/Insta360 MP4.

            If the app crashes on launch with UnsatisfiedLinkError, the installed ABI does not match.
            Library default is arm64 only; for emulator run from repo root:
              ./gradlew :sample-app:installDebug -PrustAbis=arm64-v8a,x86_64
            """.trimIndent()
    }

    private fun processUri(uri: Uri) {
        tvLog.text = "Copying and parsing…"
        Thread {
            val sb = StringBuilder()
            try {
                try {
                    Class.forName("io.github.telemetryparser.TelemetryParser")
                } catch (e: ClassNotFoundException) {
                    throw IllegalStateException(
                        "telemetry-parser classes not on classpath. " +
                            "Use implementation(project(\":android\")) or a fat AAR with classes.jar.",
                        e
                    )
                }

                val temp = File(cacheDir, "telemetry_sample.mp4")
                contentResolver.openInputStream(uri)?.use { input ->
                    FileOutputStream(temp).use { out -> input.copyTo(out) }
                } ?: throw IllegalStateException("Could not open $uri")

                val path = temp.absolutePath
                sb.append("Path: $path\n\n")

                TelemetryParser().use { parser ->
                    parser.open(path)
                    sb.append("Camera: ${parser.getCameraType()}\n")
                    sb.append("Duration: %.3f s\n".format(parser.getDuration().toDouble()))

                    parser.getVideoInfo()?.let { vi ->
                        sb.append(
                            "VideoInfo: %d frames @ %d/%d fps\n".format(
                                vi.frameCount,
                                vi.fpsNumerator,
                                vi.fpsDenominator
                            )
                        )
                    }

                    val gps = parser.getGpsData()
                    sb.append("GPS points: ${gps.size}\n")
                    if (gps.isNotEmpty()) {
                        sb.append("  first: ${gps.first()}\n")
                    }

                    val acc = parser.getAccelerometerData()
                    val gyr = parser.getGyroscopeData()
                    sb.append("Accel samples: ${acc.size}\n")
                    sb.append("Gyro samples: ${gyr.size}\n")
                    if (acc.isNotEmpty()) sb.append("  accel[0]: ${acc.first()}\n")
                    if (gyr.isNotEmpty()) sb.append("  gyro[0]: ${gyr.first()}\n")

                    val lens = parser.getLensMetadataJson()
                    if (lens != null) {
                        sb.append("Lens JSON length: ${lens.length} chars\n")
                    }
                }
                temp.delete()
            } catch (e: UnsatisfiedLinkError) {
                sb.clear()
                sb.append("UnsatisfiedLinkError (native .so missing for this ABI):\n${e.message}\n\n")
                sb.append("Build the library with matching ABIs, e.g.:\n")
                sb.append("  ./gradlew :sample-app:assembleDebug -PrustAbis=arm64-v8a,x86_64\n")
                e.printStackTrace()
            } catch (e: Exception) {
                sb.append("Error: ${e::class.java.simpleName}: ${e.message}\n")
                e.printStackTrace()
            }

            runOnUiThread { tvLog.text = sb.toString() }
        }.start()
    }
}
