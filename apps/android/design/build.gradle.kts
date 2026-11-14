plugins {
    alias(libs.plugins.android.library)
    alias(libs.plugins.kotlin.compose)
}

android {
    namespace = "com.codync.android.design"
    compileSdk = 37
    defaultConfig.minSdk = 29
    buildFeatures.compose = true
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
}
android.testOptions.unitTests.all {
    it.systemProperty("codync.orbs", rootProject.file("../ios/Kit/Tests/CodyncKitTests/Fixtures/thinking-orbs-golden.json").absolutePath)
}
kotlin.compilerOptions.jvmTarget.set(org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17)

dependencies {
    implementation(platform(libs.compose.bom))
    implementation(libs.compose.ui)
    implementation(libs.compose.foundation)
    implementation(libs.compose.material3)
    implementation(libs.lifecycle.compose)
    implementation(libs.activity.compose)
    api(libs.backdrop)
    testImplementation(libs.junit)
    testImplementation(libs.serialization.json)
    api(libs.kyant.shapes)
}
