//! What a DWG-read drawing keeps beyond what `cadcraft-dwg` translates, by way of CADCraft's own
//! DXF reader, and the document a drawing with no DWG source is written into.

use std::sync::Arc;

use cadcraft_doc::{Drawing, EntityKind};

use crate::{IoError, Result, Version, dxf_read, dxf_write};

/// Give a drawing read from `dwg` what CADCraft's DXF reader takes from the file's objects:
/// the unmodelled objects and their references (written back by a DXF save, read by dynamic
/// block editing), layer states, table, multiline and multileader styles, plot styles,
/// constraints, and the entities `cadcraft-dwg` reads as unmodelled (tables, multilines, OLE
/// frames, other programs' proxies) and block clips. Derived from the file's DWG through
/// acadrust's DXF writer, with the entities CADCraft reads itself left out.
pub(crate) fn complete(d: &mut Drawing, dwg: &[u8]) -> Result<()> {
    let dxf = cadcraft_dwg::kept_dxf(dwg).map_err(IoError::Format)?;
    let k = dxf_read::read(&dxf).map_err(|e| IoError::Format(format!("{e} (in the objects of this DWG)")))?;
    let mut foreign = (*k.foreign).clone();
    foreign.dwg_source.clone_from(&d.foreign.dwg_source);
    foreign.version.clone_from(&d.foreign.version);
    d.foreign = Arc::new(foreign);
    d.layer_states = k.layer_states;
    d.table_styles = if k.table_styles.is_empty() { std::mem::take(&mut d.table_styles) } else { k.table_styles };
    if !k.mline_styles.is_empty() {
        d.mline_styles = k.mline_styles;
    }
    if !k.mleader_styles.is_empty() {
        d.mleader_styles = k.mleader_styles;
    }
    d.plot_style_tables = k.plot_style_tables;
    d.constraints = k.constraints;
    d.parametric = k.parametric;
    // Entities: what the DXF reader makes of an unmodelled one, and block references' clips.
    let take = |store: &mut cadcraft_doc::EntityStore, from: &cadcraft_doc::EntityStore| {
        let hs: Vec<cadcraft_doc::Handle> = store
            .iter()
            .filter(|e| matches!(e.kind, EntityKind::Unknown(_) | EntityKind::Insert(_) | EntityKind::Image(_) | EntityKind::Viewport(_)))
            .map(|e| e.handle)
            .collect();
        for h in hs {
            let Some(src) = from.get(h) else { continue };
            store.modify(h, |e| match (&mut e.kind, &src.kind) {
                (EntityKind::Unknown(_), k) => e.kind = k.clone(),
                (EntityKind::Insert(i), EntityKind::Insert(s)) => i.clip.clone_from(&s.clip),
                (EntityKind::Image(i), EntityKind::Image(s)) => {
                    i.path.clone_from(&s.path);
                    i.name.clone_from(&s.name);
                }
                (EntityKind::Viewport(v), EntityKind::Viewport(s)) => v.layer_colors.clone_from(&s.layer_colors),
                _ => {}
            });
        }
    };
    let model = k.model.clone();
    take(&mut d.model, &model);
    for l in &mut d.layouts {
        if let Some(kl) = k.layouts.iter().find(|x| x.name.eq_ignore_ascii_case(&l.name)) {
            take(&mut l.entities, &kl.entities);
        }
    }
    let names: Vec<String> = d.blocks.keys().cloned().collect();
    for n in names {
        let Some(kb) = k.blocks.get(&n) else { continue };
        if let Some(b) = d.blocks.get_mut(&n) {
            let b = Arc::make_mut(b);
            take(&mut b.entities, &kb.entities);
            b.array.clone_from(&kb.array);
            b.annotative |= kb.annotative;
        }
    }
    // The entities kept only as the file had them, as the DXF reader made them.
    if let Some(src) = &d.foreign.dwg_source {
        let kept = all(d).filter(|e| src.kept.contains_key(&e.handle)).map(|e| (e.handle, e.clone())).collect();
        let mut src = src.clone();
        src.kept = Arc::new(kept);
        let mut f = (*d.foreign).clone();
        f.dwg_source = Some(src);
        d.foreign = Arc::new(f);
    }
    Ok(())
}

fn all(d: &Drawing) -> impl Iterator<Item = &cadcraft_doc::Entity> {
    std::iter::once(&d.model).chain(d.layouts.iter().map(|l| &l.entities)).chain(d.blocks.values().map(|b| &b.entities)).flat_map(|s| s.iter().map(|e| e.as_ref()))
}

/// The acadrust document a drawing with no DWG source is written into: CADCraft's DXF of it, as
/// acadrust reads it.
pub(crate) fn seed(d: &Drawing, version: Version) -> Result<cadcraft_dwg::acadrust::CadDocument> {
    let dxf = dxf_write::dwg_bridge(d, version);
    std::panic::catch_unwind(move || cadcraft_dwg::acadrust::DxfReader::from_reader(std::io::Cursor::new(dxf.into_bytes())).and_then(|r| r.read()))
        .map_err(|_| IoError::Format("the DWG writer failed on this drawing".into()))?
        .map_err(|e| IoError::Format(format!("DWG: {e}")))
}
