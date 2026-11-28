#!/bin/sh
# Installs the launchd user agent that runs `runner.py tick` every 600 seconds.
# It installs nothing else. uninstall.sh removes it.
# The plist keeps the state path of the time of the install. Run this again after
# an update of the checkout, so that the plist gets the current default.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
repo=$(dirname "$here")
state="${YANTRA_STATE:-$HOME/.local/state/yantra}"
agents=$HOME/Library/LaunchAgents
target=$agents/com.yantra.tick.plist
domain=gui/$(id -u)

mkdir -p "$state" "$agents"
echo "state directory: \"$state\""

sed -e "s|__REPO__|$repo|g" -e "s|__HOME__|$HOME|g" -e "s|__STATE__|$state|g" \
	"$here/com.yantra.tick.plist" >"$target"
plutil -lint "$target"
echo "wrote $target"
echo "the plist has YANTRA_STATE=\"$state\""

if launchctl bootout "$domain/com.yantra.tick" 2>/dev/null; then
	echo "unloaded the old com.yantra.tick"
fi
launchctl bootstrap "$domain" "$target"
echo "loaded com.yantra.tick into $domain"
echo "it runs: python3 \"$repo/yantra/runner.py\" tick, each 600 seconds"
echo "log of the ticks: \"$state/tick.log\""
echo "log of each run: \"$state/runs/<branch>-<time>/run.log\""
