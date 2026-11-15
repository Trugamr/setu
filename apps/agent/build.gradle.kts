plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.compose)
    id("com.android.compose.screenshot") version "0.0.1-alpha16"
}

/** The agent's version, `-Psetu.version=1.2.3` (a release passes its tag), or 0.1.0. */
val agentVersion: String = providers.gradleProperty("setu.version").getOrElse("0.1.0")

/** 1.2.3 becomes 10203, so each release's version code is higher than the last. */
fun agentVersionCode(version: String): Int {
    val (major, minor, patch) = version.substringBefore('-').split('.').map(String::toInt)
    return major * 10000 + minor * 100 + patch
}

android {
    namespace = "dev.trugamr.setu"
    compileSdk = 37

    defaultConfig {
        applicationId = "dev.trugamr.setu"
        // Quest 3 runs Horizon OS on Android 14 (API 34).
        minSdk = 31
        targetSdk = 34
        versionCode = agentVersionCode(agentVersion)
        versionName = agentVersion
    }

    // The release key comes from the environment (CI decodes it from a secret); without it the
    // release APK is built unsigned.
    val keystore = providers.environmentVariable("SETU_KEYSTORE").orNull
    signingConfigs {
        if (keystore != null) {
            create("release") {
                storeFile = file(keystore)
                storePassword = providers.environmentVariable("SETU_KEYSTORE_PASSWORD").get()
                keyAlias = providers.environmentVariable("SETU_KEY_ALIAS").get()
                keyPassword = providers.environmentVariable("SETU_KEY_PASSWORD").get()
            }
        }
    }

    buildTypes {
        release {
            // R8 takes the APK from about 37 MB to 4 MB, under the 25 MiB a Cloudflare static
            // asset may be, so the browser installer can serve it next to the page.
            isMinifyEnabled = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
            signingConfig = signingConfigs.findByName("release")
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    buildFeatures {
        compose = true
    }

    // Renders @PreviewTest previews to PNGs on the JVM: ./gradlew updateDebugScreenshotTest.
    experimentalProperties["android.experimental.enableScreenshotTest"] = true

    testOptions {
        unitTests.all {
            // Byte fixtures shared with the desktop app's protocol tests.
            it.systemProperty("setu.protocolFixtures", rootDir.resolve("../../protocol/fixtures").canonicalPath)
        }
    }
}

dependencies {
    implementation(libs.androidx.core.ktx)
    implementation(libs.androidx.activity.compose)
    implementation(libs.androidx.lifecycle.runtime.compose)
    implementation(platform(libs.androidx.compose.bom))
    implementation(libs.androidx.compose.material3)
    implementation(libs.androidx.compose.ui.tooling.preview)
    debugImplementation(libs.androidx.compose.ui.tooling)
    // The agent's adb client, for launching the capture server through this headset's adbd.
    implementation(libs.libadb)
    // Makes the self-signed certificate that wireless debugging's TLS presents for the adb key.
    implementation(libs.bouncycastle.pkix)

    testImplementation(libs.junit)
    screenshotTestImplementation("com.android.tools.screenshot:screenshot-validation-api:0.0.1-alpha16")
    screenshotTestImplementation(libs.androidx.compose.ui.tooling)
}
