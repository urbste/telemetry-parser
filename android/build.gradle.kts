import java.io.File

plugins {
    id("com.android.library")
    id("org.jetbrains.kotlin.android")
}

val rustAbisList: List<String> =
    ((project.findProperty("rustAbis") as String?) ?: "arm64-v8a")
        .split(",")
        .map { it.trim() }
        .filter { it.isNotEmpty() }

val rustProfileProp: String =
    (project.findProperty("rustProfile") as String?) ?: "release"

android {
    namespace = "io.github.telemetryparser"
    compileSdk = 34
    ndkVersion = "26.3.11579264"

    defaultConfig {
        minSdk = 21
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
        consumerProguardFiles("consumer-rules.pro")

        ndk {
            abiFilters.clear()
            abiFilters += rustAbisList
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = false
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_11
        targetCompatibility = JavaVersion.VERSION_11
    }

    kotlinOptions {
        jvmTarget = "11"
    }
}

dependencies {
    implementation("androidx.core:core-ktx:1.12.0")
    implementation("androidx.annotation:annotation:1.7.1")

    testImplementation("junit:junit:4.13.2")
    androidTestImplementation("androidx.test.ext:junit:1.1.5")
    androidTestImplementation("androidx.test.espresso:espresso-core:3.5.1")
}

// Plain File for configuration-cache (avoid capturing script `android` accessor in the Exec task)
val androidNdkDirForRust: File = android.ndkDirectory
val androidNdkVersionPinned: String = android.ndkVersion

val jniLibsRootDir: File = File(project.projectDir, "src/main/jniLibs")

// Build Rust library via cargo-ndk before assembling
val buildRustLib = tasks.register<Exec>("buildRustLib") {
    val projectRootDir = rootProject.projectDir
    val jniLibsPath = jniLibsRootDir

    inputs.file(File(projectRootDir, "Cargo.toml"))
    inputs.file(File(projectRootDir, "Cargo.lock"))
    inputs.dir(File(projectRootDir, "src"))

    rustAbisList.forEach { abi ->
        outputs.file(File(jniLibsPath, "$abi/libtelemetry_parser.so"))
    }

    doFirst {
        check(androidNdkDirForRust.exists()) {
            "Android NDK not found at ${androidNdkDirForRust.absolutePath}. " +
                "Install NDK $androidNdkVersionPinned via Android Studio SDK Manager, or set ANDROID_NDK_HOME."
        }
    }

    // cargo-ndk 4.x: cargo args follow directly (no "--" separator)
    val args = mutableListOf(
        "cargo", "ndk",
        "-o", jniLibsPath.absolutePath,
    )
    rustAbisList.forEach { abi ->
        args += listOf("-t", abi)
    }
    args += "build"
    args += "-p"
    args += "telemetry-parser"
    if (rustProfileProp == "release") {
        args += "--release"
    } else {
        args += "--profile"
        args += rustProfileProp
    }
    commandLine(args)

    workingDir = projectRootDir
}

// Ensure Rust library is built before any Android compilation
tasks.named("preBuild") {
    dependsOn(buildRustLib)
}
