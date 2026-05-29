$ErrorActionPreference = "Stop"

$root = Split-Path -Parent $MyInvocation.MyCommand.Path
Set-Location $root

Write-Host "Building SnartNet Android shell..."

if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    throw "cargo not found. Install Rust from https://rustup.rs"
}

rustup target add aarch64-linux-android | Out-Null

$gradlew = Join-Path $root "android\gradlew.bat"
if (Test-Path $gradlew) {
    Push-Location "android"
    try {
        .\gradlew.bat :app:assembleDebug
    }
    finally {
        Pop-Location
    }
} elseif (Get-Command gradle -ErrorAction SilentlyContinue) {
    Push-Location "android"
    try {
        gradle :app:assembleDebug
    }
    finally {
        Pop-Location
    }
} else {
    throw "Gradle not found. Install Gradle or add Android Gradle Wrapper files."
}

Write-Host "Android build complete: android/app/build/outputs/apk/debug/app-debug.apk"
