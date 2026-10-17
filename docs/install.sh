#!/bin/bash

set -euo pipefail

GITHUB_REPO="christianhelle/airdash"
BINARY_NAME="airdash"
APP_ID="com.christianhelle.airdash"
INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"
DATA_DIR="${XDG_DATA_HOME:-$HOME/.local/share}"
DESKTOP_FILE="$DATA_DIR/applications/$APP_ID.desktop"
ICON_FILE="$DATA_DIR/icons/hicolor/256x256/apps/$APP_ID.png"
REQUESTED_VERSION="${VERSION:-}"
UNINSTALL=false

log_info() {
  echo "[INFO] $1" >&2
}

log_success() {
  echo "[OK] $1" >&2
}

log_warning() {
  echo "[WARN] $1" >&2
}

log_error() {
  echo "[ERROR] $1" >&2
}

# The Rust target triple that names the release archive.
detect_target() {
  local os
  local arch
  os=$(uname -s | tr '[:upper:]' '[:lower:]')
  arch=$(uname -m)

  # Report the real CPU when the shell runs under Rosetta.
  if [[ "$os" == "darwin" ]] && [[ "$(sysctl -n hw.optional.arm64 2>/dev/null || echo 0)" == "1" ]]; then
    arch="aarch64"
  fi

  case "$arch" in
  x86_64 | amd64)
    arch="x86_64"
    ;;
  aarch64 | arm64)
    arch="aarch64"
    ;;
  *)
    log_error "Unsupported architecture: $arch"
    exit 1
    ;;
  esac

  case "$os" in
  linux*)
    echo "${arch}-unknown-linux-gnu"
    ;;
  darwin*)
    echo "${arch}-apple-darwin"
    ;;
  *)
    log_error "Unsupported operating system: $os. On Windows, download the zip from https://github.com/$GITHUB_REPO/releases"
    exit 1
    ;;
  esac
}

check_dependencies() {
  local deps=("curl" "tar")

  for dep in "${deps[@]}"; do
    if ! command -v "$dep" >/dev/null 2>&1; then
      log_error "Required dependency '$dep' not found. Please install it first."
      exit 1
    fi
  done
}

ensure_directory() {
  local dir="$1"

  if [[ -d "$dir" ]]; then
    return
  fi

  log_info "Creating directory: $dir"

  if mkdir -p "$dir" 2>/dev/null; then
    return
  fi

  if command -v sudo >/dev/null 2>&1; then
    sudo mkdir -p "$dir"
    return
  fi

  log_error "Cannot create directory '$dir'."
  exit 1
}

# Copies next to the target and renames over it, so an AirDash that is running keeps working.
install_binary() {
  local source_binary="$1"
  local target_binary="$INSTALL_DIR/$BINARY_NAME"
  local staged_binary="$INSTALL_DIR/.$BINARY_NAME.new"

  log_info "Installing $BINARY_NAME to $INSTALL_DIR..."

  if [[ -w "$INSTALL_DIR" ]]; then
    cp "$source_binary" "$staged_binary"
    chmod 755 "$staged_binary"
    mv -f "$staged_binary" "$target_binary"
    return
  fi

  if ! command -v sudo >/dev/null 2>&1; then
    log_error "Cannot write to '$INSTALL_DIR' and sudo is not available."
    exit 1
  fi

  log_warning "Installing with sudo because '$INSTALL_DIR' is not writable by the current user."
  sudo cp "$source_binary" "$staged_binary"
  sudo chmod 755 "$staged_binary"
  sudo mv -f "$staged_binary" "$target_binary"
}

# Lets app launchers (such as the Omarchy launcher) list AirDash with its icon.
install_desktop_entry() {
  local source_dir="$1"
  local source_desktop="$source_dir/$APP_ID.desktop"
  local source_icon="$source_dir/$APP_ID.png"

  if [[ ! -f "$source_desktop" ]] || [[ ! -f "$source_icon" ]]; then
    log_warning "This release has no desktop entry, so AirDash will not show up in app launchers."
    return
  fi

  log_info "Adding AirDash to the app launcher..."
  mkdir -p "$(dirname "$DESKTOP_FILE")" "$(dirname "$ICON_FILE")"
  cp "$source_icon" "$ICON_FILE"
  # Point the entry at the installed binary, so it starts even when INSTALL_DIR is not on the session PATH.
  sed "s|^Exec=.*|Exec=\"$INSTALL_DIR/$BINARY_NAME\"|" "$source_desktop" >"$DESKTOP_FILE"
  chmod 644 "$ICON_FILE" "$DESKTOP_FILE"
  refresh_desktop_caches
}

refresh_desktop_caches() {
  if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database -q "$DATA_DIR/applications" 2>/dev/null || true
  fi
  if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache -q -t "$DATA_DIR/icons/hicolor" 2>/dev/null || true
  fi
}

download_url() {
  local target="$1"
  local archive_name="$BINARY_NAME-$target.tar.gz"

  if [[ -n "$REQUESTED_VERSION" ]]; then
    echo "https://github.com/$GITHUB_REPO/releases/download/$REQUESTED_VERSION/$archive_name"
  else
    echo "https://github.com/$GITHUB_REPO/releases/latest/download/$archive_name"
  fi
}

download_and_install() {
  local target="$1"
  local archive_name="$BINARY_NAME-$target.tar.gz"
  local temp_dir
  local archive_path
  local archive_entries
  local entry
  local binary_entry_count
  local -a binary_entries

  temp_dir=$(mktemp -d)
  archive_path="$temp_dir/$archive_name"

  # Removes itself after running, so it doesn't fire again when later functions return
  trap 'rm -rf "$temp_dir"; trap - RETURN' RETURN

  log_info "Downloading $archive_name..."

  if ! curl -fsSL --proto '=https' --tlsv1.2 -o "$archive_path" "$(download_url "$target")"; then
    log_error "Failed to download '$archive_name' from GitHub Releases."
    if [[ -n "$REQUESTED_VERSION" ]]; then
      log_error "Check that release '$REQUESTED_VERSION' exists: https://github.com/$GITHUB_REPO/releases"
    fi
    return 1
  fi

  archive_entries=$(tar -tzf "$archive_path")
  if [[ -z "$archive_entries" ]]; then
    log_error "The downloaded archive is empty or unreadable."
    return 1
  fi

  if grep -Eq '(^/|(^|/)\.\.(/|$))' <<<"$archive_entries"; then
    log_error "The downloaded archive contains unsafe paths."
    return 1
  fi

  binary_entries=()
  while IFS= read -r entry; do
    [[ -n "$entry" ]] && binary_entries+=("$entry")
  done < <(awk -F/ -v name="$BINARY_NAME" '$NF == name && $0 !~ /\/$/ { print }' <<<"$archive_entries")
  binary_entry_count=${#binary_entries[@]}
  if [[ "$binary_entry_count" -ne 1 ]]; then
    log_error "Expected exactly one '$BINARY_NAME' entry in '$archive_name', found $binary_entry_count."
    return 1
  fi

  if ! tar -tvzf "$archive_path" "${binary_entries[0]}" | awk 'NR == 1 { exit (substr($1, 1, 1) == "-" ? 0 : 1) } END { if (NR == 0) exit 1 }'; then
    log_error "The '$BINARY_NAME' entry in '$archive_name' is not a regular file."
    return 1
  fi

  log_info "Extracting archive..."
  mkdir "$temp_dir/files"
  if ! tar -xzf "$archive_path" -C "$temp_dir/files"; then
    log_error "Failed to extract '$archive_name'."
    return 1
  fi

  ensure_directory "$INSTALL_DIR"
  install_binary "$temp_dir/files/${binary_entries[0]}"

  if [[ "$(uname -s)" == "Linux" ]]; then
    install_desktop_entry "$(dirname "$temp_dir/files/${binary_entries[0]}")"
  fi
}

uninstall() {
  local target_binary="$INSTALL_DIR/$BINARY_NAME"

  log_info "Removing AirDash..."

  if [[ -e "$target_binary" ]]; then
    if [[ -w "$INSTALL_DIR" ]]; then
      rm -f "$target_binary"
    else
      sudo rm -f "$target_binary"
    fi
  fi
  rm -f "$DESKTOP_FILE" "$ICON_FILE"
  refresh_desktop_caches

  log_success "AirDash has been removed. Your settings and saved dashboard are kept."
}

show_next_steps() {
  if [[ "$(uname -s)" == "Linux" ]]; then
    log_info "Start AirDash from your app launcher (Super + Space on Omarchy), or run: $BINARY_NAME"
  else
    log_info "Start AirDash by running: $BINARY_NAME"
  fi

  if ! command -v "$BINARY_NAME" >/dev/null 2>&1; then
    log_warning "'$INSTALL_DIR' is not on PATH for the current shell."
    log_info "Run '$INSTALL_DIR/$BINARY_NAME' or add '$INSTALL_DIR' to your PATH."
  fi
}

show_usage() {
  cat <<EOF
AirDash installation script

Usage: $0 [OPTIONS]

Options:
  -d, --dir DIR          Set the installation directory (default: \$HOME/.local/bin)
  -v, --version VERSION  Install a specific release tag instead of the latest GitHub Release
  -u, --uninstall        Remove AirDash, its desktop entry and its icon
  -h, --help             Show this help message

Environment variables:
  INSTALL_DIR            Installation directory (default: \$HOME/.local/bin)
  VERSION                Release tag to install

Examples:
  curl -fsSL https://christianhelle.com/airdash/install | bash
  curl -fsSL https://christianhelle.com/airdash/install | bash -s -- --version v0.1.0
  curl -fsSL https://christianhelle.com/airdash/install | bash -s -- --uninstall
EOF
}

main() {
  while [[ $# -gt 0 ]]; do
    case "$1" in
    -h | --help)
      show_usage
      exit 0
      ;;
    -d | --dir)
      INSTALL_DIR="$2"
      shift 2
      ;;
    -v | --version)
      REQUESTED_VERSION="$2"
      shift 2
      ;;
    -u | --uninstall)
      UNINSTALL=true
      shift
      ;;
    *)
      log_error "Unknown option: $1"
      show_usage
      exit 1
      ;;
    esac
  done

  if [[ "$UNINSTALL" == true ]]; then
    uninstall
    exit 0
  fi

  log_info "Starting AirDash installation..."
  check_dependencies

  local target
  target=$(detect_target)
  log_info "Detected platform: $target"
  log_info "Installing release: ${REQUESTED_VERSION:-latest}"

  download_and_install "$target"
  log_success "Installation complete."
  show_next_steps
}

main "$@"
