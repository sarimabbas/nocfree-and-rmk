#!/bin/sh
set -eu
cd "$(dirname "$0")"
build_dir=$(mktemp -d "${TMPDIR:-/tmp}/nocfree-recovery-gate.XXXXXX")
trap 'rm -rf "$build_dir"' EXIT HUP INT TERM
${CC:-cc} -std=c11 -Wall -Wextra -Werror -pedantic -fsanitize=address,undefined \
  recovery_gate.c test_gate.c -o "$build_dir/test-gate"
"$build_dir/test-gate"
