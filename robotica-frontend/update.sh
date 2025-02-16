#!/bin/sh
set -e

if ! git diff-index --quiet HEAD --; then
	echo "ERROR: Something has changed" >&2
	exit 1
fi

npm --prefix robotica-frontend update
rm -rf robotica-frontend/node_modules
hash="$(prefetch-npm-deps ./robotica-frontend/package-lock.json)"
echo "updated npm dependency hash: $hash" >&2
echo "$hash" >npm-deps-hash

git add robotica-frontend/package-lock.json
git add npm-deps-hash
