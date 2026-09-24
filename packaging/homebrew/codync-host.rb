# Homebrew formula for the leepokai/homebrew-codync tap.
# After tagging `host-vX.Y.Z`, update `version` and the four sha256 values from the
# release's *.sha256 files, then copy this file to Formula/codync-host.rb in the tap.
class CodyncHost < Formula
  desc "Runs your coding-agent bots (Claude Code, Codex, OpenCode…) for the Codync app"
  homepage "https://github.com/leepokai/Codync"
  version "2.0.0"
  license "MIT"

  base = "https://github.com/leepokai/Codync/releases/download/host-v#{version}/codync-host-#{version}"

  on_macos do
    on_arm do
      url "#{base}-aarch64-apple-darwin.tar.gz"
      sha256 "REPLACE_WITH_SHA256"
    end
    on_intel do
      url "#{base}-x86_64-apple-darwin.tar.gz"
      sha256 "REPLACE_WITH_SHA256"
    end
  end

  on_linux do
    on_arm do
      url "#{base}-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "REPLACE_WITH_SHA256"
    end
    on_intel do
      url "#{base}-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "REPLACE_WITH_SHA256"
    end
  end

  def install
    bin.install "codync-host"
  end

  def caveats
    <<~EOS
      Start the host in the background and pair your iPhone:
        codync-host install
        codync-host pair
    EOS
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/codync-host --version")
  end
end
