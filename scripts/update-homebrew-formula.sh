#!/bin/sh
set -eu

# Regenerates packaging/homebrew/Formula/aegis.rb for a release tag by
# downloading the four <asset>.sha256 sidecars published alongside the
# GitHub Release, plus the checked-in THIRD_PARTY_NOTICES.md asset. Fails
# closed if any sidecar or the notice is missing, or a checksum is not
# exactly 64 hex characters. The release assets are raw single-file
# binaries (not archives), so every url is emitted with `using: :nounzip`
# to stop Homebrew from trying to decompress them.
#
# THIRD_PARTY_NOTICES.md has no `.sha256` sidecar (it is a checked-in file
# attached to the Release as-is, not a build matrix artifact), so its
# checksum is computed locally from the downloaded asset instead of fetched
# from a sidecar.

usage() {
  printf 'Usage: %s vX.Y.Z\n' "$0" >&2
}

if [ "$#" -ne 1 ]; then
  usage
  exit 2
fi

tag="$1"
case "$tag" in
  v*) ;;
  *)
    printf 'release tag must start with v: %s\n' "$tag" >&2
    exit 2
    ;;
esac

version="${tag#v}"
repo="${AEGIS_RELEASE_REPO:-IliasAlmerekov/aegis-shellguard}"
base_url="https://github.com/${repo}/releases/download/${tag}"
out="${AEGIS_HOMEBREW_FORMULA:-packaging/homebrew/Formula/aegis.rb}"
tmp_dir="$(mktemp -d)"

cleanup() {
  rm -rf "$tmp_dir"
}
trap cleanup EXIT INT TERM

validate_sha256() {
  asset="$1"
  url="$2"
  checksum="$3"

  if ! printf '%s\n' "$checksum" | grep -Eq '^[[:xdigit:]]{64}$'; then
    printf 'invalid sha256 for %s from %s\n' "$asset" "$url" >&2
    exit 1
  fi
}

fetch_sha() {
  sidecar="$1"
  asset="${sidecar%.sha256}"
  checksum_file="${tmp_dir}/${sidecar}"
  url="${base_url}/${sidecar}"

  curl -fsSL "$url" -o "$checksum_file"
  checksum="$(awk '{print $1}' "$checksum_file")"
  validate_sha256 "$asset" "$url" "$checksum"
  printf '%s' "$checksum"
}

# Mirrors select_checksum_tool() in scripts/install.sh: same tool-detection
# order and bare-name contract, so both scripts pick sha256sum over shasum
# identically; each script appends the shasum -a 256 form at its own call site.
select_checksum_tool() {
  if command -v sha256sum >/dev/null 2>&1; then
    printf 'sha256sum\n'
    return
  fi

  if command -v shasum >/dev/null 2>&1; then
    printf 'shasum\n'
    return
  fi

  printf 'no supported checksum tool found (need sha256sum or shasum)\n' >&2
  exit 1
}

compute_sha256() {
  file="$1"

  checksum_tool="$(select_checksum_tool)"
  case "$checksum_tool" in
    sha256sum)
      sha256sum "$file" | awk '{print $1}'
      ;;
    shasum)
      shasum -a 256 "$file" | awk '{print $1}'
      ;;
  esac
}

fetch_notices_sha() {
  asset="THIRD_PARTY_NOTICES.md"
  notices_file="${tmp_dir}/${asset}"
  url="${base_url}/${asset}"

  curl -fsSL "$url" -o "$notices_file"
  checksum="$(compute_sha256 "$notices_file")"
  validate_sha256 "$asset" "$url" "$checksum"
  printf '%s' "$checksum"
}

linux_x86_64="$(fetch_sha aegis-linux-x86_64.sha256)"
linux_aarch64="$(fetch_sha aegis-linux-aarch64.sha256)"
macos_x86_64="$(fetch_sha aegis-macos-x86_64.sha256)"
macos_aarch64="$(fetch_sha aegis-macos-aarch64.sha256)"
third_party_notices="$(fetch_notices_sha)"

mkdir -p "$(dirname "$out")"

cat > "$out" <<EOF
class Aegis < Formula
  desc "Heuristic shell guardrail for AI agent command execution"
  homepage "https://github.com/IliasAlmerekov/aegis-shellguard"
  version "${version}"
  license "MIT"

  on_macos do
    if Hardware::CPU.arm?
      url "${base_url}/aegis-macos-aarch64", using: :nounzip
      sha256 "${macos_aarch64}"
    else
      url "${base_url}/aegis-macos-x86_64", using: :nounzip
      sha256 "${macos_x86_64}"
    end
  end

  on_linux do
    if Hardware::CPU.arm?
      url "${base_url}/aegis-linux-aarch64", using: :nounzip
      sha256 "${linux_aarch64}"
    else
      url "${base_url}/aegis-linux-x86_64", using: :nounzip
      sha256 "${linux_x86_64}"
    end
  end

  resource "third_party_notices" do
    url "${base_url}/THIRD_PARTY_NOTICES.md"
    sha256 "${third_party_notices}"
  end

  def install
    bin.install Dir["aegis-*"].first => "aegis"

    resource("third_party_notices").stage do
      (share/"doc/aegis").install "THIRD_PARTY_NOTICES.md"
    end
  end

  def caveats
    <<~EOS
      Homebrew installs the aegis binary and its third-party notices
      (share/doc/aegis/THIRD_PARTY_NOTICES.md).

      To install supported Claude Code and Codex hooks after installation:
        aegis install-hooks --all

      To enable shell-proxy mode for tools that launch commands through \$SHELL -c:
        aegis setup-shell

      To undo shell-proxy setup:
        aegis setup-shell --remove

      Native Windows shells are not supported; use Aegis from WSL2 on Windows.
    EOS
  end

  test do
    assert_match "brew-test", shell_output("#{bin}/aegis -c 'echo brew-test'")
  end
end
EOF

printf '%s updated\n' "$out"