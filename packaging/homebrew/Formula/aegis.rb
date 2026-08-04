class Aegis < Formula
  desc "Heuristic shell guardrail for AI agent command execution"
  homepage "https://github.com/IliasAlmerekov/aegis-shellguard"
  version "0.6.2"
  license "MIT"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/IliasAlmerekov/aegis-shellguard/releases/download/v0.6.2/aegis-macos-aarch64", using: :nounzip
      sha256 "c17189eb9a823cd14bc3df19fe7388080a53470b0969a39398cccc0c06e4acf7"
    else
      url "https://github.com/IliasAlmerekov/aegis-shellguard/releases/download/v0.6.2/aegis-macos-x86_64", using: :nounzip
      sha256 "239958ea5fb24fd9d00b4c53c89670ffd803e032700909759146bebb8ac117f5"
    end
  end

  on_linux do
    if Hardware::CPU.arm?
      url "https://github.com/IliasAlmerekov/aegis-shellguard/releases/download/v0.6.2/aegis-linux-aarch64", using: :nounzip
      sha256 "60488cbc84054689c50124b37fcc5a21e31065a4ecbaca0d4c9f4583ab431d28"
    else
      url "https://github.com/IliasAlmerekov/aegis-shellguard/releases/download/v0.6.2/aegis-linux-x86_64", using: :nounzip
      sha256 "912dbc29774e8564ee2ae8fc33a560a96687e001bba19bbe757701a673016dca"
    end
  end

  resource "third_party_notices" do
    url "https://github.com/IliasAlmerekov/aegis-shellguard/releases/download/v0.6.2/THIRD_PARTY_NOTICES.md"
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
