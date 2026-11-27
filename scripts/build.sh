#!/usr/bin/env bash
set -euo pipefail

# FileFusion local build & verification script (Linux / macOS / Git Bash).
# Mirrors `.github/workflows/build-apps.yml` sequentially to stay memory-safe.

APP="all"
BUILD_MODE="debug"
INCLUDE_TESTS="true"
ENABLE_OPTIONAL_CODECS="false"
DRY_RUN="false"

usage() {
  cat <<'EOF'
Usage: scripts/build.sh [OPTIONS]
  --app <target>              all|desktop|windows|linux|macos|mobile|android|ios|web|cli|api (default: all)
  --build-mode <mode>         release|debug (default: debug)
  --include-tests <bool>      true|false (default: true)
  --enable-optional-codecs    Enable optional Tier 2 codec verification (default: false)
  --dry-run                   Resolve local vs CI-delegated targets without building
  -h, --help                  Show this help message
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --app) APP="${2:-}"; shift 2 ;;
    --build-mode) BUILD_MODE="${2:-}"; shift 2 ;;
    --include-tests) INCLUDE_TESTS="${2:-}"; shift 2 ;;
    --enable-optional-codecs) ENABLE_OPTIONAL_CODECS="true"; shift ;;
    --dry-run) DRY_RUN="true"; shift ;;
    -h|--help) usage; exit 0 ;;
    *) echo "Error: Unknown argument '$1'" >&2; usage; exit 2 ;;
  esac
done

case "$APP" in
  all|desktop|windows|linux|macos|mobile|android|ios|web|cli|api) ;;
  *) echo "Error: Invalid --app '$APP'" >&2; exit 2 ;;
esac
case "$BUILD_MODE" in
  release|debug) ;;
  *) echo "Error: Invalid --build-mode '$BUILD_MODE'" >&2; exit 2 ;;
esac
case "$INCLUDE_TESTS" in
  true|false) ;;
  *) echo "Error: Invalid --include-tests '$INCLUDE_TESTS' (must be true|false)" >&2; exit 2 ;;
esac

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
cd "$REPO_ROOT"

UNAME_S="$(uname -s)"
case "$UNAME_S" in
  Linux*)  HOST_OS="linux"; HOST_ARCH_SUFFIX="x64" ;;
  Darwin*) HOST_OS="macos"; HOST_ARCH_SUFFIX="arm64" ;;
  MINGW*|MSYS*|CYGWIN*) HOST_OS="windows"; HOST_ARCH_SUFFIX="x64" ;;
  *)       HOST_OS="linux"; HOST_ARCH_SUFFIX="x64" ;;
esac

is_unavailable_locally() {
  local target="$1"
  case "$HOST_OS" in
    linux)   [[ "$target" == "windows" || "$target" == "macos" || "$target" == "ios" ]] ;;
    windows) [[ "$target" == "linux" || "$target" == "macos" || "$target" == "ios" ]] ;;
    macos)   [[ "$target" == "windows" || "$target" == "linux" ]] ;;
  esac
}

REQUESTED=()
case "$APP" in
  all)     REQUESTED=(cli windows linux macos android ios web api) ;;
  desktop) REQUESTED=(windows linux macos) ;;
  mobile)  REQUESTED=(android ios) ;;
  *)       REQUESTED=("$APP") ;;
esac

BUILDABLE=()
SKIPPED_CROSS=()
for t in "${REQUESTED[@]}"; do
  if is_unavailable_locally "$t"; then
    SKIPPED_CROSS+=("$t")
  else
    BUILDABLE+=("$t")
  fi
done

echo "=== FileFusion Local Build (${HOST_OS} | app=${APP} | mode=${BUILD_MODE} | tests=${INCLUDE_TESTS} | optional_codecs=${ENABLE_OPTIONAL_CODECS}) ==="
if [[ ${#SKIPPED_CROSS[@]} -gt 0 ]]; then
  echo "Warning: Unavailable on local '${HOST_OS}' host (delegate to GitHub Actions build-apps.yml): ${SKIPPED_CROSS[*]}" >&2
  if [[ ${#BUILDABLE[@]} -eq 0 ]]; then
    echo "Error: Target '${APP}' cannot be cross-compiled locally on '${HOST_OS}'." >&2
    exit 1
  fi
fi

if [[ "$DRY_RUN" == "true" ]]; then
  echo "[DRY-RUN] Buildable targets: ${BUILDABLE[*]}"
  if [[ ${#SKIPPED_CROSS[@]} -gt 0 ]]; then
    echo "[DRY-RUN] Delegated to CI: ${SKIPPED_CROSS[*]}"
  fi
  exit 0
fi

write_metadata_and_checksums() {
  local stage_dir="$1"
  local artifact_name="$2"
  local target="$3"
  local stage_status="$4"
  local git_sha
  git_sha="$(git rev-parse --short HEAD 2>/dev/null || echo uncommitted)"

  cp LICENSE LICENSES.md "$stage_dir/"
  cat > "$stage_dir/build-metadata.json" <<EOF
{
  "artifact_name": "${artifact_name}",
  "target": "${target}",
  "host_os": "${HOST_OS}",
  "build_mode": "${BUILD_MODE}",
  "enable_optional_codecs": ${ENABLE_OPTIONAL_CODECS},
  "stage_status": "${stage_status}",
  "git_sha": "${git_sha}"
}
EOF
  (
    cd "$stage_dir"
    if command -v sha256sum >/dev/null 2>&1; then
      find . -type f ! -name "SHA256SUMS.txt" -print0 | LC_ALL=C sort -z | xargs -0 sha256sum > SHA256SUMS.txt
    else
      find . -type f ! -name "SHA256SUMS.txt" -print0 | LC_ALL=C sort -z | xargs -0 shasum -a 256 > SHA256SUMS.txt
    fi
  )
}

echo "-> Running Rust workspace format & clippy checks..."
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
if [[ "$INCLUDE_TESTS" == "true" ]]; then
  echo "-> Running Rust workspace tests..."
  cargo test --workspace --locked
fi

PROFILE_FLAG=""
TARGET_SUBDIR="debug"
FLUTTER_MODE="--debug"
if [[ "$BUILD_MODE" == "release" ]]; then
  PROFILE_FLAG="--release"
  TARGET_SUBDIR="release"
  FLUTTER_MODE="--release"
fi

BUILT_ARTIFACTS=()
for t in "${BUILDABLE[@]}"; do
  echo "-> Building target: ${t} (${BUILD_MODE})"
  case "$t" in
    cli)
      ART_NAME="filefusion-cli-${HOST_OS}-${HOST_ARCH_SUFFIX}"
      STAGE_DIR="dist/local/${ART_NAME}"
      mkdir -p "$STAGE_DIR"
      if [[ -f "apps/cli/Cargo.toml" ]]; then
        cargo build -p filefusion-cli $PROFILE_FLAG --locked
        BIN_NAME="filefusion"
        [[ "$HOST_OS" == "windows" ]] && BIN_NAME="filefusion.exe"
        cp "target/${TARGET_SUBDIR}/${BIN_NAME}" "$STAGE_DIR/"
        STATUS="full-cli-binary"
      else
        cargo build -p file-core $PROFILE_FLAG --locked
        cp "target/${TARGET_SUBDIR}/libfile_core.rlib" "$STAGE_DIR/"
        STATUS="stage-0-core-anchor"
      fi
      write_metadata_and_checksums "$STAGE_DIR" "$ART_NAME" "$t" "$STATUS"
      BUILT_ARTIFACTS+=("$ART_NAME")
      ;;
    api)
      ART_NAME="filefusion-api-${HOST_OS}-${HOST_ARCH_SUFFIX}"
      STAGE_DIR="dist/local/${ART_NAME}"
      mkdir -p "$STAGE_DIR"
      if [[ -f "apps/api/Cargo.toml" ]]; then
        cargo build -p filefusion-api $PROFILE_FLAG --locked
        BIN_NAME="filefusion-api"
        [[ "$HOST_OS" == "windows" ]] && BIN_NAME="filefusion-api.exe"
        cp "target/${TARGET_SUBDIR}/${BIN_NAME}" "$STAGE_DIR/"
        STATUS="full-api-binary"
      else
        cargo build -p file-core $PROFILE_FLAG --locked
        cp "target/${TARGET_SUBDIR}/libfile_core.rlib" "$STAGE_DIR/"
        STATUS="stage-0-core-anchor"
      fi
      write_metadata_and_checksums "$STAGE_DIR" "$ART_NAME" "$t" "$STATUS"
      BUILT_ARTIFACTS+=("$ART_NAME")
      ;;
    windows|linux|macos|android|ios)
      if [[ "$t" == "android" ]]; then
        ART_NAME="filefusion-android-apk"
        APP_DIR="apps/mobile"
      elif [[ "$t" == "ios" ]]; then
        ART_NAME="filefusion-ios-build"
        APP_DIR="apps/mobile"
      elif [[ "$t" == "macos" ]]; then
        ART_NAME="filefusion-macos-arm64"
        APP_DIR="apps/desktop"
      else
        ART_NAME="filefusion-${t}-x64"
        APP_DIR="apps/desktop"
      fi
      STAGE_DIR="dist/local/${ART_NAME}"
      mkdir -p "$STAGE_DIR"
      if [[ -f "${APP_DIR}/pubspec.yaml" ]]; then
        (
          cd "$APP_DIR"
          flutter pub get
          dart analyze
          [[ "$INCLUDE_TESTS" == "true" ]] && flutter test
          case "$t" in
            windows)
              flutter build windows $FLUTTER_MODE
              cp -R build/windows/x64/runner/*/ "../../${STAGE_DIR}/"
              ;;
            linux)
              flutter build linux $FLUTTER_MODE
              cp -R "build/linux/"*"/${TARGET_SUBDIR}/bundle/"* "../../${STAGE_DIR}/"
              ;;
            macos)
              flutter build macos $FLUTTER_MODE
              cp -R build/macos/Build/Products/*/*.app "../../${STAGE_DIR}/"
              ;;
            android)
              flutter build apk $FLUTTER_MODE
              cp build/app/outputs/flutter-apk/*.apk "../../${STAGE_DIR}/"
              ;;
            ios)
              flutter build ios $FLUTTER_MODE --no-codesign
              cp -R build/ios/iphoneos/*.app "../../${STAGE_DIR}/" 2>/dev/null || cp -R build/ios/iphonesimulator/*.app "../../${STAGE_DIR}/"
              ;;
          esac
        )
        STATUS="full-flutter-build"
      else
        cargo build -p file-core $PROFILE_FLAG --locked
        cp "target/${TARGET_SUBDIR}/libfile_core.rlib" "$STAGE_DIR/"
        STATUS="stage-0-core-anchor-pending-stage-7"
      fi
      write_metadata_and_checksums "$STAGE_DIR" "$ART_NAME" "$t" "$STATUS"
      BUILT_ARTIFACTS+=("$ART_NAME")
      ;;
    web)
      ART_NAME="filefusion-web"
      STAGE_DIR="dist/local/${ART_NAME}"
      mkdir -p "${STAGE_DIR}/wasm"
      if rustup target list --installed 2>/dev/null | grep -qx "wasm32-unknown-unknown"; then
        cargo build -p file-core --target wasm32-unknown-unknown --no-default-features --features wasm $PROFILE_FLAG --locked
        cp "target/wasm32-unknown-unknown/${TARGET_SUBDIR}/libfile_core.rlib" "${STAGE_DIR}/wasm/"
        WASM_STATUS="wasm32-compiled"
      else
        cargo check -p file-core --no-default-features --features wasm --locked
        cargo build -p file-core --no-default-features --features wasm $PROFILE_FLAG --locked
        cp "target/${TARGET_SUBDIR}/libfile_core.rlib" "${STAGE_DIR}/wasm/"
        WASM_STATUS="host-wasm-feature-checked"
      fi
      if [[ -f "apps/web/package.json" ]]; then
        pnpm --dir apps/web install --frozen-lockfile
        pnpm --dir apps/web check
        [[ "$INCLUDE_TESTS" == "true" ]] && pnpm --dir apps/web test
        pnpm --dir apps/web build
        cp -R apps/web/build/* "${STAGE_DIR}/"
        STATUS="full-web-build-${WASM_STATUS}"
      else
        STATUS="stage-0-${WASM_STATUS}-pending-stage-8"
      fi
      write_metadata_and_checksums "$STAGE_DIR" "$ART_NAME" "$t" "$STATUS"
      BUILT_ARTIFACTS+=("$ART_NAME")
      ;;
  esac
done

echo "=== Build Complete ==="
echo "Built locally (dist/local/): ${BUILT_ARTIFACTS[*]}"
if [[ ${#SKIPPED_CROSS[@]} -gt 0 ]]; then
  echo "Delegated to CI (unavailable on ${HOST_OS}): ${SKIPPED_CROSS[*]}"
fi
