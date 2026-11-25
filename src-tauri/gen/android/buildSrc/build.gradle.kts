plugins {
    `kotlin-dsl`
}

gradlePlugin {
    plugins {
        create("pluginsForCoolKids") {
            id = "rust"
            implementationClass = "RustPlugin"
        }
    }
}

repositories {
    google()
    mavenCentral()
}

dependencies {
    compileOnly(gradleApi())
    implementation("com.android.tools.build:gradle:9.3.1")
    // buildSrc's classpath is visible to every build script and is loaded
    // before the buildscript classpath, so the Kotlin plugin reachable from
    // here is the one whose compiler runs over :app. AGP brings an older
    // Kotlin plugin along, and a compiler older than the crash SDK's cannot
    // read its AAR metadata, so this dependency makes the two agree.
    implementation("org.jetbrains.kotlin:kotlin-gradle-plugin:2.4.10")
}

