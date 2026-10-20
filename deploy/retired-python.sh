#!/usr/bin/env bash
# Sourced by provisioning, never by an agent. #180 retires the known Python
# installation paths. These checks run before build or provisioning and
# never delete an executable that an existing agent may still need.
# The caller supplies die(). Arbitrarily renamed programs are not identified
# by their contents: binary selection remains the operator's deployment fact.

refuse_python_binary() { # PATH DESCRIPTION
  case "$1" in
    */pyworker|*/python-spu|*/python-spu.pyz|*/python-spu-*.pyz|*/python-spu/*)
      die "$2 names retired Python executable $1 (#180). Unload with the installed admin, take down affected agents by deploy/HowToDeployANewAgent.md section 7, then follow deploy/REDEPLOY.md's Python retirement procedure."
      ;;
  esac
}

refuse_python_install() { # PREFIX BIN_DIR
  local retired
  # lstat counts dangling links and refuses an unreadable lookup. A missing
  # name alone is absence. Python here is deployment tooling, not an organ.
  retired=$(python3 - "$1/python-spu" "$2/pyworker" <<'PY'
import os
import sys

for path in sys.argv[1:]:
    try:
        os.lstat(path)
    except FileNotFoundError:
        continue
    except OSError as error:
        sys.exit(f"cannot inspect retired installation {path}: {error}")
    print(path)
PY
  ) || die "the retired Python installation could not be inspected"
  [ -z "$retired" ] || die "retired Python installation remains (#180): $retired. Follow deploy/REDEPLOY.md's Python retirement procedure before installing or provisioning."
}
