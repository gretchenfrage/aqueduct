#!/usr/bin/env bash

set -e
SCRIPT_DIR=$(cd -P -- "$(dirname -- "$0")" && pwd -P)
cd "${SCRIPT_DIR}/.."

export SEG_QUEUE_TEST_OUTER_ITERATIONS=1
export SEG_QUEUE_TEST_INNER_ITERATIONS=50
export MIRIFLAGS="-Zmiri-disable-isolation"

cargo +nightly miri test
