class Aegis < Formula
  desc "Heuristic shell guardrail for AI agent command execution"
  homepage "https://github.com/IliasAlmerekov/aegis-shellguard"
  version "0.7.0"
  license "MIT"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/IliasAlmerekov/aegis-shellguard/releases/download/v0.7.0/aegis-macos-aarch64", using: :nounzip
      sha256 "aa7d6565630bf42c4607ef8b10c1bc0b6d261463f256a848585637a439737587"
    else
      url "https://github.com/IliasAlmerekov/aegis-shellguard/releases/download/v0.7.0/aegis-macos-x86_64", using: :nounzip
      sha256 "ff2a5dcd08e9b6a2d043c8425676630f505ed923d470160f21e5ed87081bee71"
    end
  end

  on_linux do
    if Hardware::CPU.arm?
      url "https://github.com/IliasAlmerekov/aegis-shellguard/releases/download/v0.7.0/aegis-linux-aarch64", using: :nounzip
      sha256 "17b42d98644c123b1f1d0c2e96018c69a7f5e12f2beda27f8e255131579b5225"
    else
      url "https://github.com/IliasAlmerekov/aegis-shellguard/releases/download/v0.7.0/aegis-linux-x86_64", using: :nounzip
      sha256 "6eafeab8ab5acce71ca76bcafef0a2a1582c2e06d51504fdd1ea400532c8cae0"
    end
  end

  resource "third_party_notices" do
    url "https://github.com/IliasAlmerekov/aegis-shellguard/releases/download/v0.7.0/THIRD_PARTY_NOTICES.md"
    sha256 "396b704f097977bd1335ad502cc29be4f443a861e445511f275020af24299d59"
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
