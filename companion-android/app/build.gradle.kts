plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("org.jetbrains.kotlin.plugin.compose")
    id("org.jetbrains.kotlin.plugin.serialization")
}

android {
    namespace = "com.fatir.companion"
    compileSdk = 35

    defaultConfig {
        applicationId = "com.fatir.companion"
        minSdk = 26
        targetSdk = 35
        versionCode = 4
        versionName = "0.3.0-agent-schedules"
    }

    buildFeatures {
        compose = true
        buildConfig = true
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    kotlinOptions {
        jvmTarget = "17"
    }
}

dependencies {
    implementation(platform("androidx.compose:compose-bom:2024.12.01"))
    implementation("androidx.activity:activity-compose:1.10.0")
    implementation("androidx.compose.ui:ui")
    implementation("androidx.compose.ui:ui-tooling-preview")
    implementation("androidx.compose.foundation:foundation")
    implementation("androidx.compose.material3:material3")
    implementation("androidx.compose.material:material-icons-extended")
    implementation("androidx.lifecycle:lifecycle-runtime-ktx:2.8.7")
    implementation("androidx.lifecycle:lifecycle-viewmodel-compose:2.8.7")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.9.0")
    implementation("org.jetbrains.kotlinx:kotlinx-serialization-json:1.7.3")
    implementation("com.squareup.okhttp3:okhttp:4.12.0")

    debugImplementation("androidx.compose.ui:ui-tooling")
}


val fatirBrandResDir = layout.buildDirectory.dir("generated/fatirBrandRes")

android.sourceSets.getByName("main").res.srcDir(fatirBrandResDir)

val prepareFatirBranding by tasks.registering(Copy::class) {
    from(rootProject.file("../ui/assets/fatir-mark.png"))
    into(fatirBrandResDir.map { it.dir("drawable-nodpi") })
    rename { "fatir_mark.png" }
}

tasks.named("preBuild").configure {
    dependsOn(prepareFatirBranding)
}
