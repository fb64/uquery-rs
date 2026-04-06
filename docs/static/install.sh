#!/usr/bin/env sh
# µQuery Installer
#
# Usage:
#   curl -"fsSL" https://install.uquery.dev | sh
#
# Environment variables:
#   UQUERY_VERSION   — install a specific version (e.g. v0.6.0), defaults to latest
#   INSTALL_DIR      — installation directory, defaults to ~/.local/bin (or /usr/local/bin for root)

set -e

REPO="fb64/uquery-rs"
BINARY="uquery"

# ── Colors ─────────────────────────────────────────────────────────────────────
# Use $(printf '\033[Xm') so variables hold actual escape bytes, not literal
# backslash strings. This works in both format-string and %s argument positions.
if [ -t 1 ] || [ -t 2 ]; then
  BOLD=$(printf '\033[1m')
  DIM=$(printf '\033[2m')
  CYAN=$(printf '\033[36m')
  GREEN=$(printf '\033[32m')
  YELLOW=$(printf '\033[33m')
  RED=$(printf '\033[31m')
  RESET=$(printf '\033[0m')
else
  BOLD='' DIM='' CYAN='' GREEN='' YELLOW='' RED='' RESET=''
fi

# ── Logging helpers ─────────────────────────────────────────────────────────────
step()    { printf "  ${CYAN}→${RESET} %s\n"   "$1"; }
success() { printf "  ${GREEN}✓${RESET} %s\n"   "$1"; }
warn()    { printf "  ${YELLOW}!${RESET} %s\n"   "$1"; }
error()   { printf "  ${RED}✗${RESET} %s\n"   "$1" >&2; exit 1; }

# ── Banner ─────────────────────────────────────────────────────────────────────
printf "\n"
printf "${CYAN}${BOLD}   ╦ ╦   ╔═╗   ╦ ╦   ╔═╗   ╦═╗   ╦ ╦  ${RESET}\n"
printf "${CYAN}${BOLD}   ║ ║   ║ ║   ║ ║   ╠═╝   ╠╦╝   ╚╦╝  ${RESET}\n"
printf "${CYAN}${BOLD}   ╠═╝   ╚╦╝   ╚═╝   ╚═╝   ╩╚═    ╩   ${RESET}\n"
printf "${CYAN}${BOLD}   ║${RESET}\n"
printf "\n"
printf "   ${DIM}Lightweight HTTP query engine · Powered by DuckDB${RESET}\n"
printf "\n"

# ── Check dependencies ─────────────────────────────────────────────────────────
if command -v curl > /dev/null 2>&1; then
  DOWNLOADER="curl"
elif command -v wget > /dev/null 2>&1; then
  DOWNLOADER="wget"
else
  error "Neither curl nor wget found. Please install one of them and retry."
fi

if ! command -v tar > /dev/null 2>&1; then
  error "tar is required but not found."
fi

# ── Detect platform ─────────────────────────────────────────────────────────────
OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
  Linux)  os="linux"  ;;
  Darwin) os="darwin" ;;
  *)      error "Unsupported operating system: ${OS}. Only Linux and macOS are supported." ;;
esac

case "$ARCH" in
  x86_64|amd64)  arch="x86_64"  ;;
  aarch64|arm64) arch="aarch64" ;;
  i686|i386)     arch="i686"    ;;
  *)             error "Unsupported architecture: ${ARCH}." ;;
esac

if [ "$os" = "darwin" ] && [ "$arch" = "i686" ]; then
  error "32-bit macOS is not supported."
fi

case "$os" in
  linux)  triple="${arch}-unknown-linux-gnu" ;;
  darwin) triple="${arch}-apple-darwin"      ;;
esac

step "Detected platform: ${BOLD}${os}/${arch}${RESET}"

# ── Resolve version ─────────────────────────────────────────────────────────────
if [ -z "$UQUERY_VERSION" ]; then
  step "Fetching latest release..."

  API_URL="https://api.github.com/repos/${REPO}/releases/latest"

  if [ "$DOWNLOADER" = "curl" ]; then
    UQUERY_VERSION="$(curl -fsSL "$API_URL" \
      | grep '"tag_name"' \
      | sed 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/')"
  else
    UQUERY_VERSION="$(wget -qO- "$API_URL" \
      | grep '"tag_name"' \
      | sed 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/')"
  fi

  [ -z "$UQUERY_VERSION" ] && error "Failed to fetch the latest release version. Check your internet connection."
fi

success "Version: ${BOLD}${UQUERY_VERSION}${RESET}"

# ── Download ────────────────────────────────────────────────────────────────────
TARBALL="${BINARY}-${triple}.tar.gz"
URL="https://github.com/${REPO}/releases/download/${UQUERY_VERSION}/${TARBALL}"

TMPDIR="$(mktemp -d)"
# shellcheck disable=SC2064
trap "rm -rf '${TMPDIR}'" EXIT

step "Downloading ${BOLD}${TARBALL}${RESET}..."

if [ "$DOWNLOADER" = "curl" ]; then
  curl -fsSL "$URL" -o "${TMPDIR}/${TARBALL}" \
    || error "Download failed. Is version ${UQUERY_VERSION} available for ${triple}?"
else
  wget -qO "${TMPDIR}/${TARBALL}" "$URL" \
    || error "Download failed. Is version ${UQUERY_VERSION} available for ${triple}?"
fi

# ── Extract ─────────────────────────────────────────────────────────────────────
step "Extracting..."
tar -xzf "${TMPDIR}/${TARBALL}" -C "$TMPDIR"
chmod +x "${TMPDIR}/${BINARY}"

# ── Install ─────────────────────────────────────────────────────────────────────
if [ -z "$INSTALL_DIR" ]; then
  if [ "$(id -u)" = "0" ]; then
    INSTALL_DIR="/usr/local/bin"
  else
    INSTALL_DIR="${HOME}/.local/bin"
  fi
fi

mkdir -p "$INSTALL_DIR"
mv "${TMPDIR}/${BINARY}" "${INSTALL_DIR}/${BINARY}"

success "Installed → ${BOLD}${INSTALL_DIR}/${BINARY}${RESET}"

# ── Pre-warm DuckDB extensions ──────────────────────────────────────────────────
step "Installing DuckDB extensions..."
if "${INSTALL_DIR}/${BINARY}" --install-extensions; then
  success "DuckDB extensions installed."
else
  warn "Extension pre-warming failed. Run '${BINARY} --install-extensions' manually."
fi

# ── PATH warning ────────────────────────────────────────────────────────────────
case ":${PATH}:" in
  *":${INSTALL_DIR}:"*) ;;
  *)
    warn "${INSTALL_DIR} is not in your PATH."
    printf "\n"
    printf "    Add this line to your shell profile (${BOLD}~/.bashrc${RESET}, ${BOLD}~/.zshrc${RESET}, …):\n"
    printf "\n"
    printf "    ${BOLD}export PATH=\"\$PATH:${INSTALL_DIR}\"${RESET}\n"
    printf "\n"
    ;;
esac

# ── Done ────────────────────────────────────────────────────────────────────────
printf "\n"
printf "  ${GREEN}${BOLD}µQuery ${UQUERY_VERSION} installed successfully!${RESET}\n"
printf "\n"
printf "  ${DIM}Quick start:${RESET}\n"
printf "    ${BOLD}uquery --help${RESET}\n"
printf "    ${BOLD}uquery --port 8080${RESET}\n"
printf "\n"
printf "  ${DIM}Documentation:${RESET}  https://uquery.dev\n"
printf "  ${DIM}Source code:${RESET}    https://github.com/${REPO}\n"
printf "\n"