#!/bin/sh
# Run a lekha tree against two SQLite test files that the coding agents never see.
# Usage: sh unseen.sh <lekha directory> [<directory for the unseen files>]
# Exit code: 0 = no wrong value, 1 = one or more wrong values, 2 = the check did not run.
set -u
[ $# -ge 1 ] && [ -f "$1/Cargo.toml" ] && [ -f "$1/tests/slt.rs" ] || { echo "unseen: give the path of a lekha directory" >&2; exit 2; }
repo=$(cd "$1" && pwd -P)
here=$(cd "$(dirname "$0")" && pwd -P)
data=${2:-${UNSEEN_DIR:-$HOME/.local/state/yantra/unseen}}
mirror=https://raw.githubusercontent.com/gregrahn/sqllogictest/c93ac416f05635ff5cc490009add936d9753d01d/test/random/expr
image=yantra-rust:1.96.1

# The real path of the directory, with no `..` and no link, decides if it is inside a checkout.
made=no
[ -d "$data" ] || { mkdir -p "$data" && made=yes; } || exit 2
data=$(cd "$data" && pwd -P)
for inside in "$repo" "$(cd "$here/../../.." && pwd -P)"; do
    case "$data/" in "$inside"/*)
        [ "$made" = yes ] && rmdir "$data" 2>/dev/null
        echo "unseen: the files must not be inside a lekha directory" >&2
        exit 2 ;;
    esac
done

sum() { shasum -a 256 "$1" | cut -d' ' -f1; }
for name in slt_good_1.test slt_good_2.test; do
    want=$(awk -v name="$name" '$2 == name { print $1 }' "$here/SHA256SUMS")
    [ -n "$want" ] || { echo "unseen: SHA256SUMS has no line for $name" >&2; exit 2; }
    [ -f "$data/$name" ] && [ "$(sum "$data/$name")" = "$want" ] && continue
    # No file, or a file that is not complete: get it again. It gets its name only when its checksum is correct.
    part=$data/$name.part.$$
    if ! curl -fsSL -o "$part" "$mirror/$name" || [ "$(sum "$part")" != "$want" ]; then
        rm -f "$part"
        echo "unseen: cannot get $name with the checksum of SHA256SUMS" >&2
        exit 2
    fi
    mv "$part" "$data/$name" || exit 2
done
docker image inspect "$image" >/dev/null 2>&1 || { echo "unseen: the Docker image $image is not there. Build it from yantra/rust.Dockerfile" >&2; exit 2; }

# Each run has an output file of its own, so a second run does not remove the evidence of the first.
out=$(mktemp "$data/run-$(date -u +%Y%m%dT%H%M%SZ)-XXXXXX") || exit 2
# The container has no network and gets both directories read-only. It works on a copy.
# Run 1 measures: the baseline of the two files is 0, so the test fails there on purpose.
# Run 2 has the measured numbers as the baseline, and each test of the harness must pass.
docker run --rm --network none --pull never -v "$repo":/src:ro -v "$data":/unseen:ro "$image" sh -c '
    cp -r /src /tmp/w && cd /tmp/w && cp /unseen/slt_good_1.test /unseen/slt_good_2.test tests/slt/random/expr/ || exit 3
    printf "random/expr/slt_good_1.test 0\nrandom/expr/slt_good_2.test 0\n" >> tests/slt/baseline.txt
    cargo test --locked --offline --test slt -- --nocapture > /tmp/first.txt 2>&1
    for name in slt_good_1.test slt_good_2.test; do
        passed=$(sed -n "s|^slt: random/expr/$name \([0-9][0-9]*\)/[0-9][0-9]*\$|\1|p" /tmp/first.txt)
        [ -n "$passed" ] || { cat /tmp/first.txt; echo "unseen: run 1 gave no score for $name"; exit 3; }
        sed -i "s|^random/expr/$name 0\$|random/expr/$name $passed|" tests/slt/baseline.txt
    done
    SLT_FAILURES=100000 cargo test --locked --offline --test slt -- --nocapture 2>&1' > "$out"
status=$?
[ "$status" = 0 ] || { echo "unseen: the test run failed with exit code $status. The directory must pass its own tests first. Read $out" >&2; exit 2; }
lines=$(grep -c '^slt: random/expr/slt_good_[12]\.test [0-9]*/[0-9]*$' "$out")
[ "$lines" = 2 ] || { echo "unseen: the test run gave no score for the two files. Read $out" >&2; exit 2; }
grep '^slt: random/expr/slt_good_[12]\.test ' "$out" | sed 's/^slt:/unseen:/'
# A wrong value: a record of an unseen file fails, and the engine gave a result that is not an error.
wrong=$(awk '/^FAIL / { unseen = ($2 ~ /slt_good_[12]\.test:/) } /^  got: / { if (unseen && $0 !~ /^  got: error:/) n++ } END { print n + 0 }' "$out")
echo "unseen: wrong values $wrong"
echo "unseen: full output in $out"
[ "$wrong" = 0 ]
