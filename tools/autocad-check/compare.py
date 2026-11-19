import os, re, sys, json
root='X:/corpus'
def read(p):
    d={'kinds':{}}
    if not os.path.exists(p): return None
    for l in open(p,encoding='latin-1'):
        a=l.split()
        if not a: continue
        if a[0]=='kind': d['kinds'][a[1]]=int(a[2])
        elif a[0] in('extmin','extmax'): d[a[0]]=[float(x) for x in a[1:]]
        else: d[a[0]]=int(a[1])
    return d
def audit(p):
    try: s=open(p,encoding='latin-1').read()
    except OSError: return None
    m=re.search(r'Total errors found (\d+)',s); return int(m.group(1)) if m else None
rows=[]
for dp,_,fs in os.walk(root+'/in'):
    for f in fs:
        if not f.lower().endswith('.dwg') or f.lower().endswith(('.rt.dwg', '.add.dwg', '.touch.dwg')): continue
        src=os.path.join(dp,f); rel=os.path.relpath(src,root+'/in').replace(os.sep,'/')
        saved=src[:-4]+'.rt.dwg'
        a=read(src+'.acad.txt'); b=read(saved+'.acad.txt')
        r={'file':rel,'audit_in':audit(src+'.acad.log'),'audit_out':audit(saved+'.acad.log'),'opened':b is not None,'diff':[]}
        if a and b:
            for k in ('model','paper','layers','blocks','anonblocks','layouts','insunits'):
                if a.get(k)!=b.get(k): r['diff'].append(f"{k} {a.get(k)}->{b.get(k)}")
            for k in sorted(set(a['kinds'])|set(b['kinds'])):
                if a['kinds'].get(k,0)!=b['kinds'].get(k,0): r['diff'].append(f"{k} {a['kinds'].get(k,0)}->{b['kinds'].get(k,0)}")
            for k in ('extmin','extmax'):
                if a.get(k) and b.get(k) and any(abs(x-y)>1e-2*max(1,abs(x)) for x,y in zip(a[k],b[k])): r['diff'].append(f"{k} {a[k]}->{b[k]}")
        rows.append(r)
json.dump(rows,open(root+'/compare.json','w'),indent=1)
n=len(rows); bad=[r for r in rows if not r['opened']]; dif=[r for r in rows if r['opened'] and r['diff']]
aud=[r for r in rows if r['opened'] and (r['audit_out'] or 0)>(r['audit_in'] or 0)]
print(f"{n} files; AutoCAD refused {len(bad)}; counts differ {len(dif)}; more audit errors than the original {len(aud)}; clean {sum(1 for r in rows if r['opened'] and not r['diff'] and (r['audit_out'] or 0)<=(r['audit_in'] or 0))}")
print('\nREFUSED'); [print(' ',r['file']) for r in bad]
print('\nDIFFER'); [print(' ',r['file'],'|','; '.join(r['diff'][:8])) for r in dif]
print('\nAUDIT'); [print(' ',r['file'],r['audit_in'],'->',r['audit_out']) for r in aud]
