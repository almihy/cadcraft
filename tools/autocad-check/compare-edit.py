# Compares AutoCAD's counts of each drawing with those of its edited saves (check-edit.sh):
#   <name>.add.dwg    a layer made, and a line, a circle and a text drawn: three more items, one more layer
#   <name>.touch.dwg  everything moved and moved back: every count as it was
import os, re, json
root = 'X:/corpus'
def read(p):
    d = {'kinds': {}}
    if not os.path.exists(p): return None
    for l in open(p, encoding='latin-1'):
        a = l.split()
        if not a: continue
        if a[0] == 'kind': d['kinds'][a[1]] = int(a[2])
        elif a[0] in ('extmin', 'extmax'): d[a[0]] = [float(x) for x in a[1:]]
        else: d[a[0]] = int(a[1])
    return d
def audit(p):
    try: s = open(p, encoding='latin-1').read()
    except OSError: return None
    m = re.search(r'Total errors found (\d+)', s); return int(m.group(1)) if m else None
out = {}
for mode in ('add', 'touch'):
    rows = []
    for dp, _, fs in os.walk(root + '/in'):
        for f in fs:
            low = f.lower()
            if not low.endswith('.dwg') or low.endswith(('.rt.dwg', '.add.dwg', '.touch.dwg')): continue
            src = os.path.join(dp, f); rel = os.path.relpath(src, root + '/in').replace(os.sep, '/')
            saved = src[:-4] + f'.{mode}.dwg'
            a = read(src + '.acad.txt'); b = read(saved + '.acad.txt')
            r = {'file': rel, 'made': os.path.exists(saved), 'opened': b is not None, 'diff': [],
                 'audit_in': audit(src + '.acad.log'), 'audit_out': audit(saved + '.acad.log')}
            if a and b:
                want = dict(a); wk = dict(a['kinds'])
                if mode == 'add':
                    want['layers'] = a['layers'] + 1
                    items = a['model'] + a['paper'] + 3
                    if b['model'] + b['paper'] != items: r['diff'].append(f"items {items - 3}+3->{b['model'] + b['paper']}")
                    if b['model'] == a['model'] + 3:
                        for k in ('LINE', 'CIRCLE', 'TEXT'): wk[k] = wk.get(k, 0) + 1
                    keys = ('layers', 'blocks', 'anonblocks', 'layouts')
                else:
                    keys = ('model', 'paper', 'layers', 'blocks', 'anonblocks', 'layouts')
                for k in keys:
                    if want.get(k) != b.get(k): r['diff'].append(f"{k} {want.get(k)}->{b.get(k)}")
                if mode == 'touch' or b['model'] == a['model'] + 3:
                    for k in sorted(set(wk) | set(b['kinds'])):
                        if wk.get(k, 0) != b['kinds'].get(k, 0): r['diff'].append(f"{k} {wk.get(k, 0)}->{b['kinds'].get(k, 0)}")
            rows.append(r)
    n = len(rows)
    unmade = [r for r in rows if not r['made']]; bad = [r for r in rows if r['made'] and not r['opened']]
    dif = [r for r in rows if r['opened'] and r['diff']]
    aud = [r for r in rows if r['opened'] and (r['audit_out'] or 0) > (r['audit_in'] or 0)]
    clean = sum(1 for r in rows if r['opened'] and not r['diff'] and (r['audit_out'] or 0) <= (r['audit_in'] or 0))
    print(f"{mode}: {n} files; not saved {len(unmade)}; AutoCAD refused {len(bad)}; counts differ {len(dif)}; AUDIT worse {len(aud)}; clean {clean}")
    for title, rs, say in (('NOT SAVED', unmade, lambda r: ''), ('REFUSED', bad, lambda r: ''), ('DIFFER', dif, lambda r: '| ' + '; '.join(r['diff'][:8])),
                           ('AUDIT', aud, lambda r: f"{r['audit_in']} -> {r['audit_out']}")):
        if rs: print(' ' + title)
        for r in rs: print('   ', r['file'], say(r))
    out[mode] = rows
json.dump(out, open(root + '/compare-edit.json', 'w'), indent=1)
