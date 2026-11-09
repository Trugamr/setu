plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.compose)
    id("com.android.compose.screenshot") version "0.0.1-alpha16"
}

android {
    namespace = "dev.trugamr.setu"
    compileSdk = 37

    defaultConfig {
        applicationId = "dev.trugamr.setu"
        // Quest 3 runs Horizon OS on Android 14 (API 34).
        minSdk = 31
        targetSdk = 34
        versionCode = 1
        versionName = "0.1.0"
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
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
