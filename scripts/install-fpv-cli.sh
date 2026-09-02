#!/usr/bin/env bash
# Install the FPV Editor CLI from a GitHub Release.

set -euo pipefail

repository="${FPV_REPOSITORY:-mroxso/FPVEditor}"
version="${FPV_VERSION:-latest}"
install_dir="${FPV_INSTALL_DIR:-$HOME/.local/bin}"

fail() {
  echo "Error: $*" >&2
  exit 1
}

case "$(uname -s)" in
  Darwin) os="macos" ;;
  Linux) os="linux" ;;
  *) fail "Unsupported operating system: $(uname -s). Use the Windows installer on Windows." ;;
esac

case "$(uname -m)" in
  arm64|aarch64) architecture="arm64" ;;
  x86_64|amd64) architecture="x64" ;;
  *) fail "Unsupported CPU architecture: $(uname -m)." ;;
esac

if [[ "$os" == "linux" && "$architecture" != "x64" ]]; then
  fail "Linux builds are currently available only for x64 CPUs."
fi

command -v curl >/dev/null 2>&1 || fail "curl is required to download fpv-cli."
command -v tar >/dev/null 2>&1 || fail "tar is required to unpack fpv-cli."

archive_name="fpv-cli-${os}-${architecture}.tar.gz"
checksums_name="fpv-cli-checksums.txt"

if [[ "$version" == "latest" ]]; then
  release_url="https://github.com/${repository}/releases/latest/download"
else
  release_url="https://github.com/${repository}/releases/download/${version}"
fi

temporary_dir="$(mktemp -d)"
trap 'rm -rf "$temporary_dir"' EXIT

echo "Downloading ${archive_name}..."
curl --fail --location --retry 3 --output "$temporary_dir/$archive_name" "$release_url/$archive_name"
curl --fail --location --retry 3 --output "$temporary_dir/$checksums_name" "$release_url/$checksums_name"

expected_checksum="$(awk -v filename="$archive_name" '$2 == filename { print $1 }' "$temporary_dir/$checksums_name")"
[[ -n "$expected_checksum" ]] || fail "No checksum for ${archive_name} was found in ${checksums_name}."

if command -v shasum >/dev/null 2>&1; then
  actual_checksum="$(shasum -a 256 "$temporary_dir/$archive_name" | awk '{print $1}')"
elif command -v sha256sum >/dev/null 2>&1; then
  actual_checksum="$(sha256sum "$temporary_dir/$archive_name" | awk '{print $1}')"
else
  fail "A SHA-256 tool (shasum or sha256sum) is required to verify the download."
fi

[[ "$actual_checksum" == "$expected_checksum" ]] || fail "Checksum verification failed for ${archive_name}."

tar -xzf "$temporary_dir/$archive_name" -C "$temporary_dir"
[[ -f "$temporary_dir/fpv" ]] || fail "The release archive does not contain the fpv executable."

mkdir -p "$install_dir"
install -m 755 "$temporary_dir/fpv" "$install_dir/fpv"

echo "Installed fpv to $install_dir/fpv"
case ":$PATH:" in
  *":$install_dir:"*) ;;
  *)
    echo "Add this directory to your PATH, then open a new terminal:"
    echo "  export PATH=\"$install_dir:\$PATH\""
    ;;
esac

missing_tools=()
command -v ffmpeg >/dev/null 2>&1 || missing_tools+=(ffmpeg)
command -v ffprobe >/dev/null 2>&1 || missing_tools+=(ffprobe)

if (( ${#missing_tools[@]} > 0 )); then
  echo
  echo "fpv was installed, but ${missing_tools[*]} was not found."
  if [[ "$os" == "macos" ]]; then
    echo "Install FFmpeg with: brew install ffmpeg"
  else
    echo "Install FFmpeg with your package manager, for example: sudo apt-get install ffmpeg"
  fi
  echo "FFmpeg and FFprobe are required for media probing and export."
fi
