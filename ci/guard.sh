#!/bin/sh
# Checks that protect the score. CI runs this before the build.
set -eu
cd "$(dirname "$0")/.."
tmp=$(mktemp -d)

# 1. The manifest lists each test file and nothing else, and each file
#    matches its checksum. A count is not enough: the paths are compared.
(
	cd tests/slt
	find . -name '*.test' | sed 's|^\./||' | LC_ALL=C sort >"$tmp/files"
	cut -c67- SHA256SUMS | LC_ALL=C sort >"$tmp/listed"
	if ! diff "$tmp/files" "$tmp/listed"; then
		echo "guard: the test files and SHA256SUMS do not list the same paths" >&2
		exit 1
	fi
	sha256sum -c --quiet SHA256SUMS
)

# 2. No dependency. The lock file has one package. The cargo commands in CI
#    use --locked, so a dependency in Cargo.toml that is not in the lock file
#    stops the build.
if [ "$(grep -c '^\[\[package\]\]' Cargo.lock)" -ne 1 ]; then
	echo "guard: Cargo.lock has more than one package" >&2
	exit 1
fi

# 3. In a pull request, a change to the engine and a change to the things
#    that measure it do not go together. Check 1 alone cannot stop a pull
#    request that edits a test file and its checksum in one change.
#    --no-renames: a rename shows its old path too, so a file that moves
#    from the measure side into src/ is seen on both sides.
#    yantra/ is on the measure side: the runner and the brief of the agents
#    check the engine, so the engine does not change with them.
if [ "${GITHUB_EVENT_NAME:-}" = pull_request ] || [ "${GITHUB_EVENT_NAME:-}" = pull_request_target ]; then
	changed=$(git -c core.quotePath=false diff --no-renames --name-only "origin/${GITHUB_BASE_REF}...HEAD")
	engine=$(printf '%s\n' "$changed" | grep -E '^(src/|Cargo\.toml$|Cargo\.lock$)' || true)
	measure=$(printf '%s\n' "$changed" | grep -E '^(tests/slt\.rs$|tests/slt/|\.github/|ci/|yantra/)' | grep -v '^tests/slt/baseline\.txt$' || true)
	if [ -n "$engine" ] && [ -n "$measure" ]; then
		echo "guard: this pull request changes the engine and the things that measure it" >&2
		printf 'engine:\n%s\nmeasure:\n%s\n' "$engine" "$measure" >&2
		exit 1
	fi
fi
echo "guard: ok"
