plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.compose)
    alias(libs.plugins.kotlin.serialization)
    alias(libs.plugins.ksp)
}

// La versión sale del Cargo.toml del workspace, para que el móvil y el PC
// digan lo mismo (ADR 0012). versionCode crece con cada versión X.Y.Z.
val appVersion: String = Regex("""(?ms)^\[workspace\.package\]$.*?^version = "([^"]+)"""")
    .find(rootDir.resolve("../Cargo.toml").readText())?.groupValues?.get(1)
    ?: error("No encuentro la versión de [workspace.package] en ../Cargo.toml")
val appVersionCode: Int = appVersion.split(".").map(String::toInt)
    .let { (major, minor, patch) -> major * 10000 + minor * 100 + patch }

// Clave de publicación (ADR 0012). Solo la tienen Actions y ai-lab; sin
// estas variables, assembleRelease sale sin firmar.
val releaseKeystore: String? = System.getenv("ATALAYA_KEYSTORE")

android {
    namespace = "dev.rubensalas.atalaya"
    compileSdk = 37

    defaultConfig {
        applicationId = "dev.rubensalas.atalaya"
        minSdk = 34
        targetSdk = 36
        versionCode = appVersionCode
        versionName = appVersion
    }

    signingConfigs {
        if (releaseKeystore != null) {
            create("publicacion") {
                storeFile = file(releaseKeystore)
                storePassword = System.getenv("ATALAYA_KEYSTORE_PASSWORD")
                keyAlias = "atalaya"
                keyPassword = System.getenv("ATALAYA_KEYSTORE_PASSWORD")
            }
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            signingConfig = signingConfigs.findByName("publicacion")
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_21
        targetCompatibility = JavaVersion.VERSION_21
    }

    buildFeatures {
        compose = true
    }
}

dependencies {
    implementation(platform(libs.compose.bom))
    implementation(libs.compose.ui)
    implementation(libs.compose.ui.tooling.preview)
    implementation(libs.compose.material3)
    implementation(libs.compose.material.icons)
    implementation(libs.activity.compose)
    implementation(libs.lifecycle.viewmodel.compose)
    implementation(libs.lifecycle.runtime.compose)
    implementation(libs.navigation.compose)
    implementation(libs.datastore.preferences)
    implementation(libs.room.runtime)
    implementation(libs.room.ktx)
    ksp(libs.room.compiler)
    implementation(libs.work.runtime)
    implementation(libs.okhttp)
    implementation(libs.serialization.json)
    implementation(libs.coroutines.android)
    implementation(libs.jsoup)
    implementation(libs.coil.compose)
    implementation(libs.coil.network.okhttp)
    debugImplementation(libs.compose.ui.tooling)

    testImplementation(libs.junit)
    testImplementation(libs.coroutines.test)
    testImplementation(libs.okhttp.mockwebserver)
}
