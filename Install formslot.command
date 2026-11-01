#!/usr/bin/env bash
cd "$(dirname "$0")" || exit 1
export FORMSLOT_NO_SETUP_OFFER=1

close_window() {
  echo
  printf 'Press Enter to close. '
  read -r _ || true
}
trap close_window EXIT

echo "Installing formslot"
echo

if [ -d cli ] && [ -d core ] && [ ! -x target/release/formslot-cli ]; then
  echo "First, building formslot. This takes a few minutes and only happens once."
  echo
  ./formslot_oc_install || {
    echo
    echo "The build did not finish. The messages above say what is missing; install it, then double-click this file again."
    exit 1
  }
fi

./formslot setup
