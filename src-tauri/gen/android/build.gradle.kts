import org.gradle.api.file.RegularFileProperty
import org.gradle.api.provider.ValueSource
import org.gradle.api.provider.ValueSourceParameters

buildscript {
    repositories {
        google()
        mavenCentral()
    }
    dependencies {
        classpath("com.android.tools.build:gradle:9.3.1")
        classpath("org.jetbrains.kotlin:kotlin-gradle-plugin:2.2.10")
    }
}

// The Kotlin half of rustls-platform-verifier ships as an AAR from the
// project's Maven archive. It must be the exact version Cargo links, so the
// dependency version is read from the workspace lockfile instead of being
// duplicated here.
abstract class RustlsVerifierVersion : ValueSource<String, RustlsVerifierVersion.Params> {
    interface Params : ValueSourceParameters {
        val lockFile: RegularFileProperty
    }

    companion object {
        const val CRATE_NAME = "rustls-platform-verifier-android"
    }

    override fun obtain(): String {
        val lines = parameters.lockFile.get().asFile.readLines()
        val nameIndex = lines.indexOfFirst { it.trim() == "name = \"$CRATE_NAME\"" }
        check(nameIndex >= 0) { "$CRATE_NAME not found in Cargo.lock" }
        return lines.drop(nameIndex + 1)
            .firstOrNull { it.trimStart().startsWith("version = ") }
            ?.substringAfter('"', "")
            ?.substringBefore('"', "")
            ?.takeIf { it.isNotEmpty() }
            ?: error("$CRATE_NAME has no version in Cargo.lock")
    }
}

val rustlsVerifierVersion = providers.of(RustlsVerifierVersion::class.java) {
    parameters.lockFile.set(layout.projectDirectory.file("../../Cargo.lock"))
}

allprojects {
    repositories {
        google()
        mavenCentral()
        maven {
            url = uri("https://github.com/rustls/rustls-platform-verifier/raw/maven-archive/android-release-support/maven/")
        }
    }

    configurations.configureEach {
        resolutionStrategy.eachDependency {
            if (requested.group == "org.rustls" && requested.name == "rustls-platform-verifier") {
                useVersion(rustlsVerifierVersion.get())
                because("the native component must match the Rust crate version")
            }
        }
    }
}

tasks.register("clean").configure {
    delete("build")
}
