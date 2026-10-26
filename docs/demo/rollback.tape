# Records the README rollback demo on real code. Nothing here is simulated.
#
# Run from the repository root, after `cargo build --release`:
#
#   vhs docs/demo/rollback.tape
#
# That writes src/assets/rollback.gif and needs no other step.
# Prerequisites: vhs (with ttyd and ffmpeg) and git on PATH, plus a release
# build of this checkout. The build must contain `aegis rollback --last`, so an
# older installed aegis does not work. The hidden setup puts the fresh build
# first on PATH, reading CARGO_TARGET_DIR when you set it.
#
# The hidden setup never touches your real ~/.aegis, git config, or any real
# repository. It points HOME at a new temp directory and builds a throwaway git
# repo there, with one uncommitted file, so the git Snapshot has work to save.

Output src/assets/rollback.gif

Set Shell "bash"
Set FontSize 18
Set Width 1200
Set Height 760
Set Padding 20
Set Theme "Dracula"
Set TypingSpeed 60ms

Hide
Type `AEGIS_BIN_DIR="${CARGO_TARGET_DIR:-$PWD/target}/release"; test -x "$AEGIS_BIN_DIR/aegis" || echo "build first: cargo build --release"` Enter
Type `export HOME="$(mktemp -d)" GIT_CONFIG_GLOBAL="$(mktemp -d)/gitconfig" PATH="$AEGIS_BIN_DIR:$PATH" PS1='$ '` Enter
Type `git config --global user.name Demo && git config --global user.email demo@aegis.dev && git config --global init.defaultBranch main` Enter
Type `mkdir -p "$HOME/demo/src" && cd "$HOME/demo"` Enter
Type `printf 'fn main() {}\n' > src/main.rs && printf 'pub fn run() {}\n' > src/lib.rs` Enter
Type `git init -q && git add -A && git commit -qm init` Enter
Type `printf 'pub fn draft() {}\n' > src/draft.rs && echo '// wip' >> src/main.rs` Enter
Type `clear` Enter
Show

Sleep 1s
Type "ls src" Enter
Sleep 1.5s

# The agent's destructive command, screened by Aegis.
Type "aegis -c 'rm -rf src'" Enter
Sleep 4s
Type "y" Enter
Sleep 1.5s

Type "ls src" Enter
Sleep 2s

# One command brings the work back.
Type "aegis rollback --last" Enter
Sleep 2.5s
Type "y" Enter
Sleep 1.5s

Type "ls src" Enter
Sleep 4s
