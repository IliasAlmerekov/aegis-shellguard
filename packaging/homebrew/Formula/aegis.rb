class Aegis < Formula
  desc "Heuristic shell guardrail for AI agent command execution"
  homepage "https://github.com/IliasAlmerekov/aegis-shellguard"
  version "0.6.4"
  license "MIT"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/IliasAlmerekov/aegis-shellguard/releases/download/v0.6.4/aegis-macos-aarch64", using: :nounzip
      sha256 "044d8405af06fd049aff3d9cabb453310c73240fe366ce40f012794c2c70d24d"
    else
      url "https://github.com/IliasAlmerekov/aegis-shellguard/releases/download/v0.6.4/aegis-macos-x86_64", using: :nounzip
      sha256 "e4575ad031ac3c72decafa7ea13e1d5f467a7d66a37543d5f956398256ea6b41"
    end
  end

  on_linux do
    if Hardware::CPU.arm?
      url "https://github.com/IliasAlmerekov/aegis-shellguard/releases/download/v0.6.4/aegis-linux-aarch64", using: :nounzip
      sha256 "66fb3135ad1081f550be28347b13310d55bbd68cb331dbce56f462e5861fa8ec"
    else
      url "https://github.com/IliasAlmerekov/aegis-shellguard/releases/download/v0.6.4/aegis-linux-x86_64", using: :nounzip
      sha256 "367492ed7453344ce54827cc14c7f8b59d1e14120443a8ec41493fdc6419f6fd"
    end
  end

  resource "third_party_notices" do
    url "https://github.com/IliasAlmerekov/aegis-shellguard/releases/download/v0.6.4/THIRD_PARTY_NOTICES.md"
    sha256 "046120a95a821791c900bf9a9b4a40279de5512eb90cc5f048ffdef1d47de404"
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

      To enable shell-proxy mode for tools that launch commands through $SHELL -c:
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
