<#
.SYNOPSIS
    FileFusion local build & verification script (Windows / PowerShell).
.DESCRIPTION
    Mirrors `.github/workflows/build-apps.yml` target selection sequentially on the
    local host (protecting 16 GB RAM CPU-only systems) and clearly reports targets
    that must be delegated to native GitHub-hosted runners.
#>
[CmdletBinding()]
param(
    [ValidateSet("all", "desktop", "windows", "linux", "macos", "mobile", "android", "ios", "web", "cli", "api")]
    [string]$App = "all",

    [ValidateSet("release", "debug")]
    [string]$BuildMode = "debug",

    [bool]$IncludeTests = $true,

    [bool]$EnableOptionalCodecs = $false,

    [switch]$DryRun
)

$ErrorActionPreference = "Stop"
$RepoRoot = Split-Path -Parent $PSScriptRoot
Push-Location $RepoRoot

function Get-HostPlatform {
    if ($IsWindows -or $env:OS -eq "Windows_NT") { return "windows" }
    if ($IsMacOS) { return "macos" }
    return "linux"
}

function Write-Utf8NoBomLf {
    param(
        [string]$Path,
        [string]$Content
    )
    $normalized = ($Content -replace "`r`n", "`n").TrimEnd("`n") + "`n"
    $utf8NoBom = New-Object System.Text.UTF8Encoding($false)
    [System.IO.File]::WriteAllText($Path, $normalized, $utf8NoBom)
}

function Write-ArtifactMetadataAndChecksums {
    param(
        [string]$StageDir,
        [string]$ArtifactName,
        [string]$Target,
        [string]$StageStatus
    )
    Copy-Item (Join-Path $RepoRoot "LICENSE") $StageDir -Force
    Copy-Item (Join-Path $RepoRoot "LICENSES.md") $StageDir -Force

    $gitSha = (git rev-parse --short HEAD 2>$null)
    if (-not $gitSha) { $gitSha = "uncommitted" }
    $meta = [ordered]@{
        artifact_name          = $ArtifactName
        target                 = $Target
        host_os                = (Get-HostPlatform)
        build_mode             = $BuildMode
        enable_optional_codecs = $EnableOptionalCodecs
        stage_status           = $StageStatus
        git_sha                = $gitSha
        generated_utc          = (Get-Date).ToUniversalTime().ToString("yyyy-MM-ddTHH:mm:ssZ")
    }
    Write-Utf8NoBomLf -Path (Join-Path $StageDir "build-metadata.json") -Content ($meta | ConvertTo-Json -Depth 4)

    $sumFile = Join-Path $StageDir "SHA256SUMS.txt"
    $baseLen = $StageDir.TrimEnd("\", "/").Length + 1
    $files = Get-ChildItem -Path $StageDir -Recurse -File | Where-Object { $_.Name -ne "SHA256SUMS.txt" } | Sort-Object FullName
    $lines = foreach ($f in $files) {
        $hash = (Get-FileHash -Path $f.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
        $rel = $f.FullName.Substring($baseLen).Replace("\", "/")
        "$hash  ./$rel"
    }
    Write-Utf8NoBomLf -Path $sumFile -Content ($lines -join "`n")
}

try {
    $hostOs = Get-HostPlatform
    $hostArchSuffix = if ($hostOs -eq "macos") { "arm64" } else { "x64" }
    Write-Host "=== FileFusion Local Build ($hostOs | app=$App | mode=$BuildMode | tests=$IncludeTests | optional_codecs=$EnableOptionalCodecs) ==="

    $unavailable = switch ($hostOs) {
        "windows" { @("linux", "macos", "ios") }
        "linux"   { @("windows", "macos", "ios") }
        "macos"   { @("windows", "linux") }
    }

    $requested = switch ($App) {
        "all"     { @("cli", "windows", "linux", "macos", "android", "ios", "web", "api") }
        "desktop" { @("windows", "linux", "macos") }
        "mobile"  { @("android", "ios") }
        default   { @($App) }
    }

    $buildable = @()
    $skippedCross = @()
    foreach ($t in $requested) {
        if ($unavailable -contains $t) {
            $skippedCross += $t
        } else {
            $buildable += $t
        }
    }

    if ($skippedCross.Count -gt 0) {
        Write-Warning "Unavailable on local '$hostOs' host (delegate to GitHub Actions build-apps.yml): $($skippedCross -join ', ')"
        if ($buildable.Count -eq 0) {
            throw "Target '$App' cannot be cross-compiled locally on '$hostOs'. Use GitHub Actions workflow_dispatch (app=$App)."
        }
    }

    if ($DryRun) {
        Write-Host "[DRY-RUN] Buildable targets: $($buildable -join ', ')"
        if ($skippedCross.Count -gt 0) {
            Write-Host "[DRY-RUN] Delegated to CI: $($skippedCross -join ', ')"
        }
        exit 0
    }

    Write-Host "-> Running Rust workspace format & clippy checks..."
    cargo fmt --all -- --check
    if ($LASTEXITCODE -ne 0) { throw "cargo fmt failed" }
    cargo clippy --workspace --all-targets -- -D warnings
    if ($LASTEXITCODE -ne 0) { throw "cargo clippy failed" }

    if ($IncludeTests) {
        Write-Host "-> Running Rust workspace tests..."
        cargo test --workspace --locked
        if ($LASTEXITCODE -ne 0) { throw "cargo test failed" }
    }

    $targetSubdir = if ($BuildMode -eq "release") { "release" } else { "debug" }
    $builtArtifacts = @()

    function Invoke-CargoBuild {
        param([string[]]$BaseArgs)
        $cmdArgs = @($BaseArgs)
        if ($BuildMode -eq "release") { $cmdArgs += "--release" }
        $cmdArgs += "--locked"
        & cargo @cmdArgs
        if ($LASTEXITCODE -ne 0) { throw "cargo $($cmdArgs -join ' ') failed" }
    }

    foreach ($t in $buildable) {
        Write-Host "-> Building target: $t ($BuildMode)"
        switch ($t) {
            "cli" {
                $artName = "filefusion-cli-$hostOs-$hostArchSuffix"
                $stageDir = Join-Path $RepoRoot "dist\local\$artName"
                New-Item -ItemType Directory -Force -Path $stageDir | Out-Null
                if (Test-Path "apps\cli\Cargo.toml") {
                    Invoke-CargoBuild @("build", "-p", "filefusion-cli")
                    $bin = if ($hostOs -eq "windows") { "filefusion.exe" } else { "filefusion" }
                    Copy-Item "target\$targetSubdir\$bin" $stageDir -Force
                    $status = "full-cli-binary"
                } else {
                    Invoke-CargoBuild @("build", "-p", "file-core")
                    Copy-Item "target\$targetSubdir\libfile_core.rlib" $stageDir -Force
                    $status = "stage-0-core-anchor"
                }
                Write-ArtifactMetadataAndChecksums -StageDir $stageDir -ArtifactName $artName -Target $t -StageStatus $status
                $builtArtifacts += $artName
            }
            "api" {
                $artName = "filefusion-api-$hostOs-$hostArchSuffix"
                $stageDir = Join-Path $RepoRoot "dist\local\$artName"
                New-Item -ItemType Directory -Force -Path $stageDir | Out-Null
                if (Test-Path "apps\api\Cargo.toml") {
                    Invoke-CargoBuild @("build", "-p", "filefusion-api")
                    $bin = if ($hostOs -eq "windows") { "filefusion-api.exe" } else { "filefusion-api" }
                    Copy-Item "target\$targetSubdir\$bin" $stageDir -Force
                    $status = "full-api-binary"
                } else {
                    Invoke-CargoBuild @("build", "-p", "file-core")
                    Copy-Item "target\$targetSubdir\libfile_core.rlib" $stageDir -Force
                    $status = "stage-0-core-anchor"
                }
                Write-ArtifactMetadataAndChecksums -StageDir $stageDir -ArtifactName $artName -Target $t -StageStatus $status
                $builtArtifacts += $artName
            }
            { $_ -in @("windows", "linux", "macos", "android", "ios") } {
                $artName = if ($t -eq "android") { "filefusion-android-apk" } elseif ($t -eq "ios") { "filefusion-ios-build" } elseif ($t -eq "macos") { "filefusion-macos-arm64" } else { "filefusion-$t-x64" }
                $appDir = if ($t -in @("android", "ios")) { "apps\mobile" } else { "apps\desktop" }
                $stageDir = Join-Path $RepoRoot "dist\local\$artName"
                New-Item -ItemType Directory -Force -Path $stageDir | Out-Null
                if (Test-Path "$appDir\pubspec.yaml") {
                    $flutterCmd = Get-Command flutter -ErrorAction SilentlyContinue
                    if (-not $flutterCmd) { throw "Flutter SDK not found on PATH." }
                    Push-Location $appDir
                    try {
                        flutter pub get
                        if ($LASTEXITCODE -ne 0) { throw "flutter pub get failed" }
                        dart analyze
                        if ($LASTEXITCODE -ne 0) { throw "dart analyze failed" }
                        if ($IncludeTests) {
                            flutter test
                            if ($LASTEXITCODE -ne 0) { throw "flutter test failed" }
                        }
                        $fFlag = if ($BuildMode -eq "release") { "--release" } else { "--debug" }
                        switch ($t) {
                            "windows" {
                                flutter build windows $fFlag
                                if ($LASTEXITCODE -ne 0) { throw "flutter build windows failed" }
                                Copy-Item "build\windows\x64\runner\*\*" $stageDir -Recurse -Force
                            }
                            "linux" {
                                flutter build linux $fFlag
                                if ($LASTEXITCODE -ne 0) { throw "flutter build linux failed" }
                                Copy-Item "build\linux\*\$targetSubdir\bundle\*" $stageDir -Recurse -Force
                            }
                            "macos" {
                                flutter build macos $fFlag
                                if ($LASTEXITCODE -ne 0) { throw "flutter build macos failed" }
                                Copy-Item "build\macos\Build\Products\*\*.app" $stageDir -Recurse -Force
                            }
                            "android" {
                                flutter build apk $fFlag
                                if ($LASTEXITCODE -ne 0) { throw "flutter build apk failed" }
                                Copy-Item "build\app\outputs\flutter-apk\*.apk" $stageDir -Force
                            }
                            "ios" {
                                flutter build ios $fFlag --no-codesign
                                if ($LASTEXITCODE -ne 0) { throw "flutter build ios failed" }
                                Copy-Item "build\ios\iphoneos\*.app" $stageDir -Recurse -Force
                            }
                        }
                    } finally {
                        Pop-Location
                    }
                    $status = "full-flutter-build"
                } else {
                    Invoke-CargoBuild @("build", "-p", "file-core")
                    Copy-Item "target\$targetSubdir\libfile_core.rlib" $stageDir -Force
                    $status = "stage-0-core-anchor-pending-stage-7"
                }
                Write-ArtifactMetadataAndChecksums -StageDir $stageDir -ArtifactName $artName -Target $t -StageStatus $status
                $builtArtifacts += $artName
            }
            "web" {
                $artName = "filefusion-web"
                $stageDir = Join-Path $RepoRoot "dist\local\$artName"
                $wasmStageDir = Join-Path $stageDir "wasm"
                New-Item -ItemType Directory -Force -Path $wasmStageDir | Out-Null
                $installedTargets = rustup target list --installed 2>$null
                if ($installedTargets -contains "wasm32-unknown-unknown") {
                    Invoke-CargoBuild @("build", "-p", "file-core", "--target", "wasm32-unknown-unknown", "--no-default-features", "--features", "wasm")
                    Copy-Item "target\wasm32-unknown-unknown\$targetSubdir\libfile_core.rlib" $wasmStageDir -Force
                    $wasmStatus = "wasm32-compiled"
                } else {
                    Write-Host "  Note: wasm32-unknown-unknown target not installed locally; running host --features wasm check."
                    & cargo check -p file-core --no-default-features --features wasm --locked
                    if ($LASTEXITCODE -ne 0) { throw "wasm feature check failed" }
                    Invoke-CargoBuild @("build", "-p", "file-core", "--no-default-features", "--features", "wasm")
                    Copy-Item "target\$targetSubdir\libfile_core.rlib" $wasmStageDir -Force
                    $wasmStatus = "host-wasm-feature-checked"
                }
                if (Test-Path "apps\web\package.json") {
                    pnpm --dir apps/web install --frozen-lockfile
                    if ($LASTEXITCODE -ne 0) { throw "pnpm install failed" }
                    pnpm --dir apps/web check
                    if ($LASTEXITCODE -ne 0) { throw "pnpm check failed" }
                    if ($IncludeTests) {
                        pnpm --dir apps/web test
                        if ($LASTEXITCODE -ne 0) { throw "pnpm test failed" }
                    }
                    pnpm --dir apps/web build
                    if ($LASTEXITCODE -ne 0) { throw "SvelteKit build failed" }
                    Copy-Item "apps\web\build\*" $stageDir -Recurse -Force
                    $status = "full-web-build-$wasmStatus"
                } else {
                    $status = "stage-0-$wasmStatus-pending-stage-8"
                }
                Write-ArtifactMetadataAndChecksums -StageDir $stageDir -ArtifactName $artName -Target $t -StageStatus $status
                $builtArtifacts += $artName
            }
        }
    }

    Write-Host "=== Build Complete ==="
    Write-Host "Built locally (dist/local/): $($builtArtifacts -join ', ')"
    if ($skippedCross.Count -gt 0) {
        Write-Host "Delegated to CI (unavailable on $hostOs): $($skippedCross -join ', ')"
    }
}
finally {
    Pop-Location
}
