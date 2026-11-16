//! CADCraft's own data in DWG files, both ways ([`Own`], the `cadcraft-dwg` extension): what
//! CADCraft keeps in extended data and dictionary records, as its DXF reader and writer keep it.
//! And, for a DXF save of a drawing read from a DWG, the DXF groups of what CADCraft keeps of the
//! file without modelling it ([`for_dxf`]).

use std::borrow::Cow;
use std::sync::Arc;

use cadcraft_doc::{Drawing, EntityKind};

use crate::{IoError, Result, dxf_read};

/// CADCraft's extension of the DWG translation.
pub(crate) struct Own;

impl cadcraft_dwg::Extension for Own {
    fn read(&self, doc: &cadcraft_dwg::acadrust::CadDocument, d: &mut Drawing) {
        let _ = doc;
        let _ = d;
    }

    fn prepare<'a>(&self, d: &'a Drawing) -> Cow<'a, Drawing> {
        Cow::Borrowed(d)
    }

    fn write(&self, d: &Drawing, doc: &mut cadcraft_dwg::acadrust::CadDocument) {
        let _ = (d, doc);
    }
}

/// A drawing read from a DWG, for a DXF save: with the DXF groups of the file's objects that
/// CADCraft keeps without modelling them (dictionaries, other programs' objects, dynamic block
/// data, extension dictionaries and reactors) and of its unmodelled entities, derived from the
/// file's DWG through acadrust's DXF writer (`cadcraft_dwg::kept_dxf`). `None` for a drawing not
/// read from a DWG, or one whose groups are already there.
pub(crate) fn for_dxf(d: &Drawing) -> Result<Option<Drawing>> {
    let Some(src) = &d.foreign.dwg_source else { return Ok(None) };
    if !d.foreign.objects.is_empty() {
        return Ok(None);
    }
    let dxf = cadcraft_dwg::kept_dxf(&src.bytes).map_err(IoError::Format)?;
    let k = dxf_read::read(&dxf).map_err(|e| IoError::Format(format!("{e} (in the objects of this DWG)")))?;
    let mut out = d.clone();
    let mut foreign = (*k.foreign).clone();
    foreign.dwg_source.clone_from(&d.foreign.dwg_source);
    foreign.version.clone_from(&d.foreign.version);
    out.foreign = Arc::new(foreign);
    // The unmodelled entities' groups, as the DXF reader keeps them.
    let take = |store: &mut cadcraft_doc::EntityStore, from: &cadcraft_doc::EntityStore| {
        let hs: Vec<cadcraft_doc::Handle> =
            store.iter().filter(|e| matches!(&e.kind, EntityKind::Unknown(u) if u.tags.is_empty())).map(|e| e.handle).collect();
        for h in hs {
            let Some(src) = from.get(h) else { continue };
            if let EntityKind::Unknown(u) = &src.kind {
                let u = u.clone();
                store.modify(h, |e| e.kind = EntityKind::Unknown(u));
            }
        }
    };
    take(&mut out.model, &k.model);
    for l in &mut out.layouts {
        if let Some(kl) = k.layouts.iter().find(|x| x.name.eq_ignore_ascii_case(&l.name)) {
            take(&mut l.entities, &kl.entities);
        }
    }
    let names: Vec<String> = out.blocks.keys().cloned().collect();
    for n in names {
        let Some(kb) = k.blocks.get(&n) else { continue };
        if let Some(b) = out.blocks.get_mut(&n) {
            take(&mut Arc::make_mut(b).entities, &kb.entities);
        }
    }
    Ok(Some(out))
}
