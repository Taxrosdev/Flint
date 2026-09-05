#!/bin/bash

# This script exists solely for build_manifest.yml, 
# and does not need to be invoked otherwise.

cargo build --release --all-features
