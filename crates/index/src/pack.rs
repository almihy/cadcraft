//! A drawing packed: everything CADCraft models of it (tables, blocks with their contents, model
//! space, sheets), serialised as MessagePack and deflated, both streamed (a large
//! office drawing serialises to 115 MB, never held whole; packed, it is 9 MB). Not what a file is
//! saved from: the data of other programs a DWG keeps (CADCraft's `foreign`) is left out, and so is
//! the file itself; the file stays where it is, and is what a save starts from.

use std::io::Write;
use std::sync::Arc;

use cadcraft_doc::{
    AnnoScale, Block, Constraint, DimStyle, Drawing, Entity, EntityStore, Group, Header, Layer, LayerState, Layout, Linetype, MLeaderStyle,
    MLineStyle, NamedView, Parametric, PlotStyleTable, TableStyle, TextStyle, Ucs, VportConfig,
};
use serde::{Deserialize, Serialize};

/// The pack's format: a pack of another is read again from its file.
pub const FORMAT: u32 = 1;
/// What a pack starts with, before its format.
const MAGIC: &[u8; 8] = b"CCDRAW\0\0";

#[derive(Serialize, Deserialize)]
struct Packed {
    header: Header,
    layers: Vec<Layer>,
    linetypes: Vec<Linetype>,
    text_styles: Vec<TextStyle>,
    dim_styles: Vec<DimStyle>,
    mleader_styles: Vec<MLeaderStyle>,
    table_styles: Vec<TableStyle>,
    mline_styles: Vec<MLineStyle>,
    blocks: Vec<(Block, Vec<Arc<Entity>>)>,
    model: Vec<Arc<Entity>>,
    layouts: Vec<(Layout, Vec<Arc<Entity>>)>,
    active_layout: Option<String>,
    views: Vec<NamedView>,
    vports: Vec<VportConfig>,
    ucss: Vec<Ucs>,
    layer_states: Vec<LayerState>,
    groups: Vec<Group>,
    plot_style_tables: Vec<PlotStyleTable>,
    constraints: Vec<Constraint>,
    parametric: Parametric,
    scales: Vec<AnnoScale>,
    handseed: u64,
    read_notes: Vec<String>,
    skipped_records: std::collections::BTreeMap<String, usize>,
}

fn entities(s: &EntityStore) -> Vec<Arc<Entity>> {
    s.iter().cloned().collect()
}

fn store(items: Vec<Arc<Entity>>) -> EntityStore {
    let mut s = EntityStore::new();
    for e in items {
        s.push(Arc::unwrap_or_clone(e));
    }
    s
}

fn packed(d: &Drawing) -> Packed {
    Packed {
        header: d.header.clone(),
        layers: d.layers.clone(),
        linetypes: d.linetypes.clone(),
        text_styles: d.text_styles.clone(),
        dim_styles: d.dim_styles.clone(),
        mleader_styles: d.mleader_styles.clone(),
        table_styles: d.table_styles.clone(),
        mline_styles: d.mline_styles.clone(),
        blocks: d.blocks.values().map(|b| ((**b).clone(), entities(&b.entities))).collect(),
        model: entities(&d.model),
        layouts: d.layouts.iter().map(|l| (l.clone(), entities(&l.entities))).collect(),
        active_layout: d.active_layout.clone(),
        views: d.views.clone(),
        vports: d.vports.clone(),
        ucss: d.ucss.clone(),
        layer_states: d.layer_states.clone(),
        groups: d.groups.clone(),
        plot_style_tables: d.plot_style_tables.clone(),
        constraints: d.constraints.clone(),
        parametric: d.parametric.clone(),
        scales: d.scales.clone(),
        handseed: d.handseed,
        read_notes: d.read_notes.clone(),
        skipped_records: d.skipped_records.clone(),
    }
}

/// The drawing packed.
pub fn pack(d: &Drawing) -> Result<Vec<u8>, String> {
    let p = packed(d);
    let failed = |e: &dyn std::fmt::Display| format!("the drawing could not be packed ({e})");
    let mut out = Vec::new();
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&FORMAT.to_le_bytes());
    // MessagePack writes a value at a time: buffered, the encoder takes them in large pieces.
    let mut z = std::io::BufWriter::with_capacity(1 << 16, flate2::write::DeflateEncoder::new(out, flate2::Compression::default()));
    rmp_serde::encode::write_named(&mut z, &p).map_err(|e| failed(&e))?;
    z.flush().map_err(|e| failed(&e))?;
    z.into_inner().map_err(|e| failed(&e))?.finish().map_err(|e| failed(&e))
}

/// A packed drawing read back. An error says the pack is not one, or of another format.
pub fn unpack(bytes: &[u8]) -> Result<Drawing, String> {
    let rest = bytes.strip_prefix(MAGIC.as_slice()).ok_or("this is not a packed drawing")?;
    let (format, rest) = rest.split_first_chunk::<4>().ok_or("this packed drawing is cut short")?;
    let format = u32::from_le_bytes(*format);
    if format != FORMAT {
        return Err(format!("this drawing was packed in format {format}, and this reads format {FORMAT}: index it again"));
    }
    let z = std::io::BufReader::with_capacity(1 << 16, flate2::read::DeflateDecoder::new(rest));
    let p: Packed = rmp_serde::from_read(z).map_err(|e| format!("this packed drawing is damaged ({e})"))?;
    let mut d = Drawing::new_metric();
    d.header = p.header;
    d.layers = p.layers;
    d.linetypes = p.linetypes;
    d.text_styles = p.text_styles;
    d.dim_styles = p.dim_styles;
    d.mleader_styles = p.mleader_styles;
    d.table_styles = p.table_styles;
    d.mline_styles = p.mline_styles;
    d.blocks = p
        .blocks
        .into_iter()
        .map(|(mut b, items)| {
            b.entities = store(items);
            (b.name.clone(), Arc::new(b))
        })
        .collect();
    d.model = store(p.model);
    d.layouts = p
        .layouts
        .into_iter()
        .map(|(mut l, items)| {
            l.entities = store(items);
            l
        })
        .collect();
    d.active_layout = p.active_layout;
    d.views = p.views;
    d.vports = p.vports;
    d.ucss = p.ucss;
    d.layer_states = p.layer_states;
    d.groups = p.groups;
    d.plot_style_tables = p.plot_style_tables;
    d.constraints = p.constraints;
    d.parametric = p.parametric;
    d.scales = p.scales;
    d.handseed = p.handseed;
    d.read_notes = p.read_notes;
    d.skipped_records = p.skipped_records;
    Ok(d)
}
