#!/bin/sh
# Stops and removes the launchd user agent of the yantra runner.
# The state directory with the logs and the run directories stays.
set -eu
target=$HOME/Library/LaunchAgents/com.yantra.tick.plist
domain=gui/$(id -u)

if launchctl bootout "$domain/com.yantra.tick" 2>/dev/null; then
	echo "unloaded com.yantra.tick from $domain"
else
	echo "com.yantra.tick was not loaded"
fi
if [ -f "$target" ]; then
	rm "$target"
	echo "removed $target"
else
	echo "there is no $target"
fi
echo "kept the state directory: \"${YANTRA_STATE:-$HOME/.local/state/yantra}\""
