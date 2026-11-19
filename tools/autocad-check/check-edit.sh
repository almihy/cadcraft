#!/usr/bin/env bash
# The edited-save check: each drawing under X:/corpus/in is opened by CADCraft, edited by a script, and saved beside
# its original, twice: add.scr (a layer, a line, a circle, a text) as <name>.add.dwg, touch.scr (everything moved and
# moved back, so every item is written from the editor's own model, not kept as the file had it) as <name>.touch.dwg.
# AutoCAD counts and audits both; compare-edit.py says what differs from what the edit should have made.
set -u
cd /x/corpus
CLI="${CLI:-/x/targets/cadcraft-corpus-cli/release/cadcraft-cli.exe}"
find in \( -name "*.add.dwg*" -o -name "*.touch.dwg*" \) -delete
: > edit.err
find in -iname "*.dwg" ! -iname "*.rt.dwg" | while IFS= read -r f; do
  for m in add touch; do
    timeout 300 "$CLI" run "X:/corpus/$f" --script-file "X:/corpus/tools/$m.scr" --save "X:/corpus/${f%.[dD][wW][gG]}.$m.dwg" > /dev/null 2> edit.tmp || { echo "== $f ($m)" >> edit.err; grep -i "error\|panick" edit.tmp | head -3 >> edit.err; }
  done
done
node tools/acad.mjs count X:/corpus/in --only '\.(add|touch)\.dwg$' --jobs 8 --limit 240 > count-edit.log 2>&1
python tools/compare-edit.py > compare-edit.txt
grep -E "^(add|touch):" compare-edit.txt
