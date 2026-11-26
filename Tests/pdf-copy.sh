#!/bin/sh
# What a copy from a PDF preview gives (ui/pdfcopy.js): a paragraph's
# lines joined into one, a word cut at a line's end put together, the
# paragraphs, a list's items and a program's lines kept apart. The lines
# are made here, set as a page sets them (Tests/pdf-copy/test.mjs); the
# part that reads them from pdf.js's words needs a web view and is not
# run. Needs node, nothing of the app. See AGENTS.md.
set -eu
cd "$(dirname "$0")/.."
exec node --test Tests/pdf-copy/test.mjs
