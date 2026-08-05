class Aegis < Formula
  desc "Heuristic shell guardrail for AI agent command execution"
  homepage "https://github.com/IliasAlmerekov/aegis-shellguard"
  version "0.6.3"
  license "MIT"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/IliasAlmerekov/aegis-shellguard/releases/download/v0.6.3/aegis-macos-aarch64", using: :nounzip
      sha256 "a2fae86aa214f7db2bcb99d2558ae05500bad19ffdf78c21c9b13facfbee1f3c"
    else
      url "https://github.com/IliasAlmerekov/aegis-shellguard/releases/download/v0.6.3/aegis-macos-x86_64", using: :nounzip
      sha256 "e55e6b5bc76674af6313e39a39fd973fa573cf313a021394bd1229676fb54c2f"
    end
  end

  on_linux do
    if Hardware::CPU.arm?
      url "https://github.com/IliasAlmerekov/aegis-shellguard/releases/download/v0.6.3/aegis-linux-aarch64", using: :nounzip
      sha256 "37bc2f97410e647c2fe85a6a2c6b53829b4b455f3159f12046b61aa4b66a446f"
    else
      url "https://github.com/IliasAlmerekov/aegis-shellguard/releases/download/v0.6.3/aegis-linux-x86_64", using: :nounzip
      sha256 "7966a756edfb285b5314fb22d89684a9d430d5e04b11e643805a5da4a9a55c06"
    end
  end

  resource "third_party_notices" do
    url "https://github.com/IliasAlmerekov/aegis-shellguard/releases/download/v0.6.3/THIRD_PARTY_NOTICES.md"
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
