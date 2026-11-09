pluginManagement {
    repositories {
        google()
        mavenCentral()
        gradlePluginPortal()
    }
}

dependencyResolutionManagement {
    repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
    repositories {
        google()
        mavenCentral()
        // libadb-android and its SPAKE2 dependency are published only on JitPack.
        exclusiveContent {
            forRepository { maven("https://jitpack.io") }
            filter { includeGroupByRegex("""com\.github\.MuntashirAkon(\..+)?""") }
        }
    }
}

rootProject.name = "setu-agent"
