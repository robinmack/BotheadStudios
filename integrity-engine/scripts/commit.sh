#!/usr/bin/env bash
#
# commit.sh — commit with a message that NEVER passes through the shell.
#
# Why this exists. Commit messages here are long and explain reasoning, so they are full of the exact
# characters a shell eats: backticks around identifiers, $, !, quotes, parentheses. Written as a heredoc
# they *look* safe and are not — an unquoted heredoc still does command substitution. That has now
# happened twice, and the second time it silently deleted the subject of a sentence from a merge commit
# on its way to `main`: "`pub mod arc;` was never declared" became " was never declared", and the commit
# was already pushed before anyone read it back.
#
# The fix is not better quoting discipline, it is removing the shell from the path. Write the message to a
# file with an editor or a file-writing tool, then hand this script the PATH. Nothing is interpolated.
#
# Usage:
#   scripts/commit.sh <message-file>              # normal commit
#   scripts/commit.sh --amend <message-file>      # reword/extend the last commit
#   scripts/commit.sh --no-trailer <message-file> # skip the Co-Authored-By trailer
#
# It also appends the assistant Co-Authored-By trailer if it is missing, so that is one less thing to
# remember, and prints the resulting subject + parents so a merge commit can be eyeballed as a merge.
set -euo pipefail

TRAILER="Co-Authored-By: Claude Opus 5 (1M context) <noreply@anthropic.com>"

amend=0
add_trailer=1
msgfile=""
for arg in "$@"; do
  case "$arg" in
    --amend)      amend=1 ;;
    --no-trailer) add_trailer=0 ;;
    -*)           echo "commit.sh: unknown flag $arg" >&2; exit 2 ;;
    *)            msgfile="$arg" ;;
  esac
done

if [[ -z "$msgfile" ]]; then
  echo "usage: scripts/commit.sh [--amend] [--no-trailer] <message-file>" >&2
  exit 2
fi
if [[ ! -s "$msgfile" ]]; then
  echo "commit.sh: '$msgfile' is missing or empty — write the message to a file first, so the shell" >&2
  echo "           never sees it. That is the entire point of this script." >&2
  exit 2
fi

# A subject line that runs on is a subject line nobody reads; git's own convention is ~50, and this repo
# routinely uses more, so warn rather than refuse.
subject="$(head -1 "$msgfile")"
if (( ${#subject} > 90 )); then
  echo "commit.sh: NOTE — subject is ${#subject} chars; consider moving detail into the body." >&2
fi

work="$(mktemp)"
trap 'rm -f "$work"' EXIT
cp "$msgfile" "$work"
if (( add_trailer )) && ! grep -qF "$TRAILER" "$work"; then
  printf '\n%s\n' "$TRAILER" >> "$work"
fi

if (( amend )); then
  git commit --quiet --amend --file "$work"
else
  git commit --quiet --file "$work"
fi

# Print what landed. `parents=[a b]` with TWO entries is the confirmation that a merge stayed a merge —
# the thing this repo checks after every integration step.
git log -1 --format='committed %h  parents=[%p]%n  %s'
