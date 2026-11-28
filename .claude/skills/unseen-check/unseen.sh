#!/bin/sh
# Run a lekha tree against two SQLite test files that the coding agents never see.
# Usage: sh unseen.sh <lekha directory> [<directory for the unseen files>]
# Exit code: 0 = no wrong value, 1 = one or more wrong values, 2 = the check did not run.
set -u
[ $# -ge 1 ] && [ -f "$1/Cargo.toml" ] && [ -f "$1/tests/slt.rs" ] || { echo "unseen: give the path of a lekha directory" >&2; exit 2; }
repo=$(cd "$1" && pwd)
here=$(cd "$(dirname "$0")" && pwd)
data=${2:-${UNSEEN_DIR:-$HOME/.local/state/yantra/unseen}}
mirror=https://raw.githubusercontent.com/gregrahn/sqllogictest/c93ac416f05635ff5cc490009add936d9753d01d/test/random/expr
image=yantra-rust:1.96.1

case "$data/" in "$repo"/*) echo "unseen: the files must not be inside the lekha directory" >&2; exit 2 ;; esac
mkdir -p "$data" || exit 2
for name in slt_good_1.test slt_good_2.test; do
    [ -f "$data/$name" ] || curl -fsSL -o "$data/$name" "$mirror/$name" || { echo "unseen: cannot get $name" >&2; exit 2; }
done
(cd "$data" && shasum -a 256 -c "$here/SHA256SUMS" >/dev/null) || { echo "unseen: a file in $data does not have the checksum of SHA256SUMS" >&2; exit 2; }
docker image inspect "$image" >/dev/null 2>&1 || { echo "unseen: the Docker image $image is not there. Build it from yantra/rust.Dockerfile" >&2; exit 2; }

# Each run has an output file of its own, so a second run does not remove the evidence of the first.
out=$data/run-$(date -u +%Y%m%dT%H%M%SZ).txt
# The container has no network and gets both directories read-only. It works on a copy.
docker run --rm --network none --pull never -v "$repo":/src:ro -v "$data":/unseen:ro "$image" sh -c '
    cp -r /src /tmp/w && cd /tmp/w && cp /unseen/slt_good_1.test /unseen/slt_good_2.test tests/slt/random/expr/ &&
    printf "random/expr/slt_good_1.test 0\nrandom/expr/slt_good_2.test 0\n" >> tests/slt/baseline.txt &&
    SLT_FAILURES=100000 cargo test --locked --offline --test slt -- --nocapture 2>&1' > "$out"
# cargo test ends with an error here on purpose: the baseline of the two files is 0.

lines=$(grep -c '^slt: random/expr/slt_good_[12]\.test [0-9]*/[0-9]*$' "$out")
[ "$lines" = 2 ] || { echo "unseen: the test run gave no score for the two files. Read $out" >&2; exit 2; }
grep '^slt: random/expr/slt_good_[12]\.test ' "$out" | sed 's/^slt:/unseen:/'
# A wrong value: a record of an unseen file fails, and the engine gave a result that is not an error.
wrong=$(awk '/^FAIL / { unseen = ($2 ~ /slt_good_[12]\.test:/) } /^  got: / { if (unseen && $0 !~ /^  got: error:/) n++ } END { print n + 0 }' "$out")
echo "unseen: wrong values $wrong"
echo "unseen: full output in $out"
[ "$wrong" = 0 ]
