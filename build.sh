#!/usr/bin/env bash
set -euo pipefail

CMD="${1:-}"

# --- GLOBAL CONFIGURATION ---
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
OUT_DIR="${ROOT_DIR}/target/generated/openapi_client"
SPEC_FILE="${ROOT_DIR}/sovd-interfaces/sovd-api.yaml"
GENERATOR_DIR="${ROOT_DIR}/target"
GENERATOR_JAR="${GENERATOR_DIR}/openapi-generator-cli.jar"
GENERATOR_VERSION="7.10.0"
GENERATOR_URL="https://repo1.maven.org/maven2/org/openapitools/openapi-generator-cli/${GENERATOR_VERSION}/openapi-generator-cli-${GENERATOR_VERSION}.jar"

# --- HELPER FUNCTIONS ---

function check_required_tools() {

    echo "==> Checking required tools on Linux..."
    local missing_tools=()

    for tool in cargo rustc java curl; do
        if ! command -v "$tool" >/dev/null 2>&1; then
            missing_tools+=("$tool")
        fi
    done

    if [[ ${#missing_tools[@]} -gt 0 ]]; then
        echo "Error: The following required tools are missing:"
        printf '  - %s\n' "${missing_tools[@]}"
        echo ""
        echo "Please install them before running this script."
        echo "On Debian/Ubuntu:"
        echo "  sudo apt install cargo openjdk-17-jre curl"
        exit 1
    fi

    echo "All required tools are available."
}

function ensure_openapi_generator() {
    echo "==> Checking OpenAPI Generator CLI..."
    mkdir -p "${GENERATOR_DIR}"

    if [[ ! -f "${GENERATOR_JAR}" ]]; then
        echo "Downloading OpenAPI generator version ${GENERATOR_VERSION}..."
        if ! curl -f -sSL "${GENERATOR_URL}" -o "${GENERATOR_JAR}"; then
            echo "Error: Failed to download OpenAPI Generator CLI."
            exit 1
        fi
        echo "Downloaded openapi-generator-cli.jar to ${GENERATOR_JAR}"
    else
        echo "Using existing OpenAPI Generator CLI: ${GENERATOR_JAR}"
    fi
}

function generate_code() {
    echo "==> Starting SOVD code generation"
    echo "Project root: ${ROOT_DIR}"

    if [[ ! -f "${SPEC_FILE}" ]]; then
        echo "Error: Spec file not found at ${SPEC_FILE}"
        exit 1
    fi

    ensure_openapi_generator
   
    # Ensure output directory exists
    mkdir -p "${OUT_DIR}"

    echo "==> Generating Rust server from ${SPEC_FILE} into ${OUT_DIR}"
    java -jar "${GENERATOR_JAR}" generate \
        -i "${SPEC_FILE}" \
        -g rust-server \
        -o "${OUT_DIR}"

    echo "Generated Rust code at ${OUT_DIR}"
}

function build_project() {
    echo "==> Building SOVD server..."
    cargo build || {
        echo "Error: Cargo build failed!"
        exit 1
    }
    echo "Build completed successfully."
}

function clean_project() {
    echo "==> Cleaning project..."
    cargo clean || echo "Warning: cargo clean failed."
    rm -rf "${OUT_DIR}" || true
    echo "Clean complete."
}

function start() {
    check_required_tools
    echo "==> Starting SOVD build process..."
    generate_code
    build_project
}

function help() {
    echo ""
    echo "Usage: $0 <command>"
    echo ""
    echo "Available commands:"
    echo "  start     Generate code and build the SOVD server"
    echo "  codegen   Run only the OpenAPI code generator"
    echo "  clean     Remove build artifacts and generated code"
    echo "  help      Show this help message"
    echo ""
}

# --- MAIN ENTRYPOINT ---
CMD="${1:-start}"   

case "$CMD" in
    start)
        start
        ;;
    clean)
        clean_project
        ;;
    help|-h|--help)
        help
        ;;
    *)
        echo "Unknown command: $CMD"
        help
        exit 1
        ;;
esac