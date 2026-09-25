#!/bin/sh
# SDK passes: --data-dir <directory> daemon run.
test "$1" = "--data-dir" && test "$3" = "daemon" && test "$4" = "run" || exit 1
touch "$2/../started"
