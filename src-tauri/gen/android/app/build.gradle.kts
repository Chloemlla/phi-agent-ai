import java.util.Properties
import org.jetbrains.kotlin.gradle.dsl.JvmTarget

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("org.jetbrains.kotlin.plugin.compose")
    id("rust")
}

// The crash-report screen comes from the Lumen Crash SDK release staged by
// `node scripts/fetch-lumen-crash.mjs`; see gradle.properties for the pin.
val lumenCrashSdkVersion: String = providers.gradleProperty("lumenCrashSdkVersion").orNull
    ?: error("lumenCrashSdkVersion is missing from gradle.properties")

val tauriProperties = Properties().apply {
    val propFile = file("tauri.properties")
    if (propFile.exists()) {
        propFile.inputStream().use { load(it) }
    }
}

// Release signing: create src-tauri/gen/android/keystore.properties
// (gitignored) with keyAlias, keyPassword, storeFile, storePassword and an
// optional storeType (for example `pkcs12`; AGP's default applies when it is
// omitted). Without the file the release build falls back to the debug
// keystore, which is fine for local testing but must never be shipped to
// users.
val keystoreProperties = Properties().apply {
    val propFile = rootProject.file("keystore.properties")
    if (propFile.exists()) {
        propFile.inputStream().use { load(it) }
    }
}
val hasReleaseKeystore = listOf(
    "keyAlias",
    "keyPassword",
    "storeFile",
    "storePassword",
).all { keystoreProperties.getProperty(it) != null }

android {
    // 37 is the Lumen Crash SDK's compile target; the app must not build against
    // an older platform than its dependency.
    compileSdk = 37
    namespace = "com.sukirya.phi"
    defaultConfig {
        manifestPlaceholders["usesCleartextTraffic"] = "false"
        applicationId = "com.sukirya.phi"
        // 26: the SDK uses java.time, which does not exist below API 26.
        minSdk = 26
        targetSdk = 36
        versionCode = tauriProperties.getProperty("tauri.android.versionCode", "1").toInt()
        versionName = tauriProperties.getProperty("tauri.android.versionName", "1.0")
    }
    signingConfigs {
        if (hasReleaseKeystore) {
            create("release") {
                keyAlias = keystoreProperties.getProperty("keyAlias")
                keyPassword = keystoreProperties.getProperty("keyPassword")
                storeFile = rootProject.file(keystoreProperties.getProperty("storeFile"))
                storePassword = keystoreProperties.getProperty("storePassword")
                keystoreProperties.getProperty("storeType")?.let { storeType = it }
            }
        }
    }
    buildTypes {
        getByName("debug") {
            manifestPlaceholders["usesCleartextTraffic"] = "true"
            isDebuggable = true
            isJniDebuggable = true
            isMinifyEnabled = false
            packaging {
                jniLibs.keepDebugSymbols.add("*/arm64-v8a/*.so")
                jniLibs.keepDebugSymbols.add("*/armeabi-v7a/*.so")
                jniLibs.keepDebugSymbols.add("*/x86/*.so")
                jniLibs.keepDebugSymbols.add("*/x86_64/*.so")
            }
        }
        getByName("release") {
            optimization {
               enable = true
            }
            // R8: shrink and obfuscate bytecode, drop unused resources.
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(
                getDefaultProguardFile("proguard-android-optimize.txt"),
                *fileTree(".") {
                  include("**/*.pro")
                  exclude("build/**")
                }.files.toTypedArray()
            )
            signingConfig = if (hasReleaseKeystore) {
                signingConfigs.getByName("release")
            } else {
                signingConfigs.getByName("debug")
            }
        }
    }
    compileOptions {
        // 17: the SDK ships Java 17 bytecode, and the Compose compiler plugin
        // targets the same level.
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    buildFeatures {
        buildConfig = true
        compose = true
    }
}

kotlin {
    compilerOptions {
        jvmTarget = JvmTarget.JVM_17
    }
}

rust {
    rootDirRel = "../../../"
}

dependencies {
    // Required by rustls-platform-verifier's Android certificate verification;
    // the version is pinned to Cargo.lock by the root build script.
    implementation("org.rustls:rustls-platform-verifier")
    implementation("androidx.webkit:webkit:1.14.0")
    implementation("androidx.appcompat:appcompat:1.7.1")
    implementation("androidx.activity:activity-ktx:1.10.1")
    implementation("com.google.android.material:material:1.12.0")
    implementation("androidx.lifecycle:lifecycle-process:2.10.0")
    // Crash reporting: `lumen-crash` carries the Compose report screen, and the
    // SDK declares both modules itself, so only the entry artifact is listed.
    // The SDK publishes its Compose dependencies without versions (they come
    // from a BOM in its own build), so the consuming build must supply the same
    // BOM or the compile classpath cannot resolve them.
    implementation(platform("androidx.compose:compose-bom:2024.12.01"))
    implementation("com.chloemlla.lumen:lumen-crash:$lumenCrashSdkVersion")
    // compose activity host for the crash screen. The SDK declares this one as
    // `implementation`, so it is not visible to this module's compile classpath.
    implementation("androidx.activity:activity-compose:1.13.0")
    testImplementation("junit:junit:4.13.2")
    androidTestImplementation("androidx.test.ext:junit:1.1.4")
    androidTestImplementation("androidx.test.espresso:espresso-core:3.5.0")
}

apply(from = file("tauri.build.gradle.kts"))
