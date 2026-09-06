#!/bin/bash

# This script installs flint into a temporary location, adds a repository, and then installs flint from there into it's final location.

set -eu

TMP_LOCATION="/tmp/flint_installer.x86_64"
INITIAL_REPOSITORY_NAME="$1"
INITIAL_REPOSITORY_URL="$2"

echo "[INFO] Using inital repository: '$INITIAL_REPOSITORY_NAME' with URL: '$INITIAL_REPOSITORY_URL'"
echo "[INFO] Downloading latest flintpkg from github releases..."

curl --follow https://github.com/Taxrosdev/Flint/releases/latest/download/flint.x86_64 -o $TMP_LOCATION

chmod 700 $TMP_LOCATION

echo "[INFO] Adding initial repository..."

$TMP_LOCATION repo add $INITIAL_REPOSITORY_NAME $INITIAL_REPOSITORY_URL 

echo "[INFO] Installing 'flint' from initial repository..."

$TMP_LOCATION install flint
# This is needed to update the quicklaunch scripts.
$TMP_LOCATION run flint -- flint update
# Cleanup temporary flintpkg
rm $TMP_LOCATION

echo "====================================================="
echo "Installed successfully!"
