#!/bin/sh
# Installs (or updates) weft for Linux and macOS:
#   curl -fsSL https://raw.githubusercontent.com/a1xon/weft/master/install.sh | sh
#   ./install.sh                  in a checkout: builds that checkout
# Takes the prebuilt binary of the latest release (curl, or `gh` while the repository is private),
# else builds from source with cargo. WEFT_BIN=<dir> installs elsewhere (default ~/.local/bin),
# WEFT_FROM_SOURCE=1 always builds.
set -eu

REPO=a1xon/weft
BIN=${WEFT_BIN:-$HOME/.local/bin}

say() { printf 'weft: %s\n' "$*"; }
die() { say "$*" >&2; exit 1; }
has() { command -v "$1" >/dev/null 2>&1; }

case $(uname -s) in
  Linux) os=linux ;;
  Darwin) os=macos ;;
  *) die "$(uname -s) is not supported (on Windows: run this inside WSL2)" ;;
esac
case $(uname -m) in
  x86_64 | amd64) arch=x86_64 ;;
  arm64 | aarch64) arch=aarch64 ;;
  *) die "$(uname -m) is not supported" ;;
esac
asset="weft-$os-$arch.tar.gz"

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT INT TERM

# a checkout this script sits in (not when piped into sh)
src=""
here=$(cd "$(dirname "$0")" 2>/dev/null && pwd || true)
if [ -n "$here" ] && [ -f "$here/Cargo.toml" ] && grep -q '^name = "weft"' "$here/Cargo.toml"; then
  src=$here
fi

fetch() {
  if has curl && curl -fsSL "https://github.com/$REPO/releases/latest/download/$asset" -o "$tmp/$asset" 2>/dev/null; then
    :
  elif has gh && gh release download --repo "$REPO" --pattern "$asset" --dir "$tmp" >/dev/null 2>&1; then
    :
  else
    return 1
  fi
  tar -xzf "$tmp/$asset" -C "$tmp" weft
}

build() {
  has cargo || die "no prebuilt weft for $os-$arch here and no Rust to build it: install Rust (https://rustup.rs), then run this again"
  if [ -z "$src" ]; then
    src=$tmp/src
    say "cloning $REPO"
    git clone -q --depth 1 "https://github.com/$REPO.git" "$src" 2>/dev/null \
      || { has gh && gh repo clone "$REPO" "$src" -- -q --depth 1; } \
      || die "cannot clone $REPO (private? log in with: gh auth login)"
  fi
  say "building in $src (a minute or two)"
  (cd "$src" && cargo build --release --locked --quiet)
  cp "$src/target/release/weft" "$tmp/weft"
}

if [ -n "$src" ] || [ -n "${WEFT_FROM_SOURCE:-}" ]; then
  build
elif fetch; then
  say "downloaded the latest release ($asset)"
else
  build
fi

# a new file moved into place, not the old one overwritten: running daemons notice the new build
# and replace themselves, and macOS kills a signed binary that is changed in place
mkdir -p "$BIN"
cp "$tmp/weft" "$BIN/.weft.new"
chmod 0755 "$BIN/.weft.new"
mv -f "$BIN/.weft.new" "$BIN/weft"
say "installed $("$BIN/weft" --version) to $BIN/weft"

# what it needs to run
warn=0
if has tmux; then
  v=$(tmux -V | sed 's/[^0-9]*\([0-9][0-9]*\)\.\([0-9][0-9]*\).*/\1 \2/')
  set -- $v
  if [ "${1:-0}" -lt 3 ] || { [ "${1:-0}" -eq 3 ] && [ "${2:-0}" -lt 2 ]; }; then
    say "tmux is $(tmux -V | cut -d' ' -f2); weft needs 3.2 or newer"; warn=1
  fi
else
  if [ "$os" = macos ]; then say "weft needs tmux: brew install tmux"; else say "weft needs tmux >= 3.2: sudo apt install tmux (or dnf / pacman)"; fi
  warn=1
fi
has git || { say "weft needs git"; warn=1; }
has claude || has codex || has agy || say "no agent CLI found (claude, codex or agy); any terminal program works, see ~/.config/weft/config.json"
case ":$PATH:" in
  *":$BIN:"*) ;;
  *) say "$BIN is not on your PATH: add  export PATH=\"$BIN:\$PATH\"  to your shell's rc file"; warn=1 ;;
esac
[ $warn = 0 ] && say "ready: cd into a project and run  weft"
exit 0
