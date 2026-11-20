class Paper < Formula
  desc "Local-first document search with exact page and line citations"
  homepage "https://github.com/arrowassassin/paperscope"
  version "0.1.0"
  license "MIT OR Apache-2.0"

  on_macos do
    if Hardware::CPU.arm?
      url "https://github.com/arrowassassin/paperscope/releases/download/v0.1.0/paperscope-aarch64-apple-darwin.tar.xz"
      sha256 "REPLACE_WITH_ACTUAL_SHA256"
    else
      url "https://github.com/arrowassassin/paperscope/releases/download/v0.1.0/paperscope-x86_64-apple-darwin.tar.xz"
      sha256 "REPLACE_WITH_ACTUAL_SHA256"
    end
  end

  on_linux do
    if Hardware::CPU.arm?
      url "https://github.com/arrowassassin/paperscope/releases/download/v0.1.0/paperscope-aarch64-unknown-linux-gnu.tar.xz"
      sha256 "REPLACE_WITH_ACTUAL_SHA256"
    else
      url "https://github.com/arrowassassin/paperscope/releases/download/v0.1.0/paperscope-x86_64-unknown-linux-gnu.tar.xz"
      sha256 "REPLACE_WITH_ACTUAL_SHA256"
    end
  end

  depends_on "sqlite"

  def install
    bin.install "paper"
    # Generate shell completions
    generate_completions_from_executable(bin/"paper", "completion")
  end

  test do
    assert_match "paper 0.1.0", shell_output("#{bin}/paper --version")
  end
end