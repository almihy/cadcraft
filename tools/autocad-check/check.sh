#!/usr/bin/env bash
# The whole check: CADCraft reads, saves and reopens every drawing under X:/corpus/in; each saved copy is put beside
# its original (so its xrefs resolve as the original's do) as <name>.rt.dwg; AutoCAD counts and audits the copies;
# the counts are compared with AutoCAD's of the originals (made once: `node acad.mjs count X:/corpus/in --only "^(?!.*\.rt\.dwg$)"`).
#   CLI=path/to/cadcraft-cli.exe bash check.sh
set -u
cd /x/corpus
CLI="${CLI:-/x/targets/cadcraft-corpus-cli/release/cadcraft-cli.exe}"
find in -name "*.rt.dwg*" -delete
rm -rf report
"$CLI" roundtrip X:/corpus/in --out X:/corpus/report --keep-saved --same-format > roundtrip.out 2> roundtrip.err
echo "roundtrip exit $?: $(grep -c 'opened' roundtrip.out) opened, $(grep -c 'failed' roundtrip.out) failed"
find in -iname "*.dwg" | while IFS= read -r f; do
  rel="${f#in/}"; s="report/saved/${rel//\//_}.roundtrip.dwg"
  [ -f "$s" ] && cp "$s" "${f%.[dD][wW][gG]}.rt.dwg"
done
node tools/acad.mjs count X:/corpus/in --only '\.rt\.dwg$' --jobs 8 --limit 240 > count-saved.log 2>&1
python tools/compare.py > compare.txt
head -1 compare.txt
