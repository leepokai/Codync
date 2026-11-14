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
        ivy {
            name = "VoskModels"
            url = uri("https://alphacephei.com/vosk/models")
            patternLayout { artifact("[artifact]-[revision].[ext]") }
            metadataSources { artifact() }
            content { includeGroup("com.alphacephei.models") }
        }
    }
}

rootProject.name = "CodyncAndroid"
include(":app", ":core", ":design")
