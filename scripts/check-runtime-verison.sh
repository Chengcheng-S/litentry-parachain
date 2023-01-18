#!/bin/bash
# set -eo we don't allow any command failed in this script.
set -eo pipefail

ROOTDIR=$(git rev-parse --show-toplevel)


# Get the runtime version and check if an upgrade is required



#function usage() {
#  echo
#  echo "Usage: $0 wasm-path"
#  echo "       wasm-path can be either local file path or https URL"
#}

#[ $# -ne 1 ] && (usage; exit 1)

function print_divider() {
  echo "------------------------------------------------------------"
}

print_divider

python3  $ROOTDIR/scripts/check-runtime-version/check-version.py

