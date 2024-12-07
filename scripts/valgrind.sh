#!/usr/bin/env bash

# TODO this script isnt very useable

set -e
SCRIPT_DIR=$(cd -P -- "$(dirname -- "$0")" && pwd -P)
cd "${SCRIPT_DIR}/.."

#export VALGRINDFLAGS="--main-stacksize=10000000"

cargo valgrind test
