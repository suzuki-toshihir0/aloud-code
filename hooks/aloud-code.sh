#!/bin/sh
# aloud-code hook runner
# バイナリが未インストールの場合、GitHub Releases から自動ダウンロードする

BINARY_NAME="aloud-code"
INSTALL_DIR="${HOME}/.local/bin"
BINARY_PATH="${INSTALL_DIR}/${BINARY_NAME}"
REPO="suzuki-toshihir0/aloud-code"

# バイナリが存在しない場合はダウンロード
if [ ! -f "${BINARY_PATH}" ]; then
    # プラットフォーム判定
    OS="$(uname -s | tr '[:upper:]' '[:lower:]')"
    ARCH="$(uname -m)"

    case "${OS}" in
        linux)  OS_NAME="linux" ;;
        darwin) OS_NAME="macos" ;;
        *)
            echo "aloud-code: unsupported OS: ${OS}" >&2
            exit 0
            ;;
    esac

    case "${ARCH}" in
        x86_64)          ARCH_NAME="x86_64" ;;
        aarch64|arm64)   ARCH_NAME="aarch64" ;;
        *)
            echo "aloud-code: unsupported arch: ${ARCH}" >&2
            exit 0
            ;;
    esac

    ASSET="${BINARY_NAME}-${OS_NAME}-${ARCH_NAME}"
    URL="https://github.com/${REPO}/releases/latest/download/${ASSET}"

    mkdir -p "${INSTALL_DIR}"
    if command -v curl >/dev/null 2>&1; then
        curl -fsSL "${URL}" -o "${BINARY_PATH}" || { echo "aloud-code: download failed" >&2; exit 0; }
    elif command -v wget >/dev/null 2>&1; then
        wget -q "${URL}" -O "${BINARY_PATH}" || { echo "aloud-code: download failed" >&2; exit 0; }
    else
        echo "aloud-code: curl or wget required" >&2
        exit 0
    fi
    chmod +x "${BINARY_PATH}"
fi

exec "${BINARY_PATH}" hook "$@"
