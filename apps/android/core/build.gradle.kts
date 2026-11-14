plugins {
    alias(libs.plugins.kotlin.jvm)
    alias(libs.plugins.kotlin.serialization)
}

kotlin {
    jvmToolchain(21)
    compilerOptions.jvmTarget.set(org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17)
}

java {
    sourceCompatibility = JavaVersion.VERSION_17
    targetCompatibility = JavaVersion.VERSION_17
}

dependencies {
    implementation(libs.coroutines.core)
    implementation(libs.serialization.json)
    implementation(libs.okhttp)
    implementation(libs.bouncycastle)
    implementation(libs.zxing)
    testImplementation(libs.junit)
    testImplementation(libs.coroutines.test)
    testImplementation(libs.mockwebserver)
}

tasks.test {
    // Consume upstream vectors in place; never copy or regenerate them for Android.
    systemProperty("codync.vectors", rootProject.file("../../docs/reference/fixtures/remote-relay-vectors.json").absolutePath)
    inputs.file(rootProject.file("../../docs/reference/fixtures/remote-relay-vectors.json"))
}
