//! CADCraft's DWG files, read and written straight through [`acadrust`] (MPL-2.0, used
//! unmodified): a DWG is read into acadrust's document and translated directly into a
//! [`cadcraft_doc::Drawing`], and a drawing is saved by translating it back and writing it with
//! acadrust's own DWG writer. There is no DXF text in between: the old bridge (`craft-dwg`) went
//! DWG → DXF → CADCraft's DXF reader and back the same way, which cost time and memory and wrote
//! DWGs AutoCAD refused.
//!
//! # Nothing unmodelled is lost
//!
//! CADCraft models a drawing's entities, tables, blocks and layouts, not everything a DWG holds
//! (dictionaries, other programs' objects, extension dictionaries, reactors, dynamic block data,
//! proxies, materials, visual styles). Those stay in the acadrust document the drawing was read
//! from, so the drawing keeps that document's source with it: the DWG bytes it was read from, in
//! [`cadcraft_doc::foreign::Foreign::dwg_source`], shared by every copy of the drawing (undo
//! snapshots clone the drawing; the bytes are behind an `Arc`). Saving reads those bytes again
//! into a fresh acadrust document (a fraction of a second; the read is deterministic) and writes
//! the drawing's state into it:
//!
//! - every object is matched by its handle (CADCraft keeps the file's handles; new objects get
//!   handles above the file's handle seed), table records and blocks by name;
//! - an object whose translation is what the file's object translates to is left exactly as it
//!   was read, so acadrust writes it back from its original record;
//! - a changed object has the properties CADCraft models written onto the original object, so its
//!   unmodelled properties (extension dictionary, reactors, materials, other programs' xdata)
//!   stay; a new one is made from scratch; one the drawing no longer has is removed.
//!
//! Keeping the bytes rather than the acadrust document itself is deliberate: the document is
//! several times larger than the file, and the native app reads untrusted files in a child
//! process ([`worker`]), whose document can't be handed back anyway. A drawing made new, or read
//! from DXF, has no source and is written into an empty document.
//!
//! # Guardrails around the third-party decoder
//!
//! The decoder allocates from sizes a (possibly damaged or hostile) file declares and can't be
//! interrupted, and an allocation failure or stack overflow ends the process, which
//! `catch_unwind` can't stop. So: the input size is checked before anything is decoded
//! ([`Limits`]); the reader, the translation and the writer run inside `catch_unwind`; and a
//! native app reads each new file first in a child process ([`worker`]), so a file that brings the
//! decoder down costs only that child. The drawing is then read in the app's own process from the
//! same bytes, which the child has just shown to read safely.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::panic::AssertUnwindSafe;

use cadcraft_doc::Drawing;

mod acis;
mod conv;
mod entity_read;
mod entity_write;
mod header;
mod mleader;
mod mline;
mod objects;
mod read;
mod styles;
mod table;
mod tables;
mod write;
mod xclip;

#[cfg(not(target_arch = "wasm32"))]
pub mod worker;

#[cfg(test)]
mod tests;

/// DWG version codes CADCraft writes, with the AutoCAD release that introduced each format.
pub const VERSIONS: &[(&str, &str)] =
    &[("AC1015", "2000"), ("AC1018", "2004"), ("AC1021", "2007"), ("AC1024", "2010"), ("AC1027", "2013"), ("AC1032", "2018")];

/// What a save leaves out of the drawing: how many objects of each type (DXF type names) the DWG
/// can't hold, empty when it holds everything.
pub type Unwritten = BTreeMap<String, usize>;

/// True when the bytes look like a DWG file (`AC10xx` magic).
pub fn is_dwg(bytes: &[u8]) -> bool {
    bytes.len() > 6 && bytes.starts_with(b"AC10") && bytes.get(4..6).is_some_and(|v| v.iter().all(u8::is_ascii_digit))
}

/// The DWG version code of a file (e.g. "AC1032").
pub fn version(bytes: &[u8]) -> Option<String> {
    is_dwg(bytes).then(|| String::from_utf8_lossy(bytes.get(0..6).unwrap_or_default()).to_string())
}

/// Size limits for reading a DWG. They exist because a damaged file, or one the reader misreads,
/// can make the reader build a drawing far larger than the file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// Largest DWG file read at all (checked before reading starts).
    pub max_dwg_bytes: usize,
    /// Most entities and objects a read may produce (checked after decoding, before the
    /// translation builds a drawing of them).
    pub max_records: usize,
}

impl Limits {
    /// 512 MB of DWG, 50 million entities and objects.
    pub const DEFAULT: Limits = Limits { max_dwg_bytes: 512 << 20, max_records: 50_000_000 };
}

/// A byte count in megabytes for messages ("9.9 MB", "1024 MB").
fn mb(n: u64) -> String {
    let m = n as f64 / f64::from(1u32 << 20);
    if m >= 100.0 { format!("{m:.0} MB") } else { format!("{m:.1} MB") }
}

/// Runs `f`, turning a panic in it into `what` as an error.
fn guarded<R>(what: &str, f: impl FnOnce() -> R) -> Result<R, String> {
    std::panic::catch_unwind(AssertUnwindSafe(f)).map_err(|_| what.to_string())
}

/// What a program adds to the translation, both ways: its own data, which a DWG carries in the
/// forms the format leaves to applications (extended data, dictionary records). CADCraft's file
/// layer (`cadcraft-io`) reads and writes its own extended data through it, as it does in DXF.
pub trait Extension {
    /// Called with the file's document once the drawing has been read from it.
    fn read(&self, _doc: &acadrust::CadDocument, _d: &mut Drawing) {}
    /// The drawing to write for `d`: `d` itself, or a copy in the form this crate writes (e.g.
    /// with the program's own kinds as standard entities carrying its extended data).
    fn prepare<'a>(&self, d: &'a Drawing, _version: acadrust::DxfVersion) -> std::borrow::Cow<'a, Drawing> {
        std::borrow::Cow::Borrowed(d)
    }
    /// The document a drawing with no DWG source is written into, when the program holds data of
    /// the drawing this crate can't translate (`None`: an empty document), with what of that data
    /// the DWG can't hold.
    fn document(&self, _d: &Drawing, _version: acadrust::DxfVersion) -> Result<Option<(acadrust::CadDocument, Unwritten)>, String> {
        Ok(None)
    }
    /// The entities of the anonymous block that draws table `t` (on layer `layer`), which a DWG
    /// keeps with the table: its cells' fills, borders and texts. Empty: none is written.
    fn table_block(&self, _d: &Drawing, _t: &cadcraft_doc::Table, _layer: &str) -> Vec<cadcraft_doc::Entity> {
        Vec::new()
    }
    /// Called once the drawing's state is in the document, before it is written.
    fn write(&self, _d: &Drawing, _doc: &mut acadrust::CadDocument) {}
}

/// No extension: the drawing as this crate translates it.
pub struct Plain;

impl Extension for Plain {}

/// Read a DWG into a drawing, with notes on what the read left out (object records the reader
/// couldn't decode), empty when the read was complete. In a native app that called
/// [`worker::use_own_worker`], the file is first read in a child process.
pub fn read(bytes: &[u8], limits: Limits) -> Result<(Drawing, Vec<String>), String> {
    read_with(bytes, limits, &Plain)
}

/// [`read`] with `ext` adding what the program keeps of the file.
pub fn read_with(bytes: &[u8], limits: Limits, ext: &dyn Extension) -> Result<(Drawing, Vec<String>), String> {
    check_size(bytes, limits)?;
    #[cfg(not(target_arch = "wasm32"))]
    worker::probe(bytes, limits)?;
    read_here_with(bytes, limits, ext)
}

/// [`read`] in this process, without the worker.
pub fn read_here(bytes: &[u8], limits: Limits) -> Result<(Drawing, Vec<String>), String> {
    read_here_with(bytes, limits, &Plain)
}

fn read_here_with(bytes: &[u8], limits: Limits, ext: &dyn Extension) -> Result<(Drawing, Vec<String>), String> {
    check_size(bytes, limits)?;
    let (mut d, notes) = guarded("the DWG reader failed on this file", || -> Result<_, String> {
        let (doc, notes) = read::decode(bytes, limits)?;
        let version = version(bytes).unwrap_or_else(|| "AC1032".into());
        let mut d = read::drawing(&doc, &version);
        d.read_notes.clone_from(&notes);
        ext.read(&doc, &mut d);
        Ok((d, notes))
    })??;
    attach_source(&mut d, bytes);
    Ok((d, notes))
}

/// Keep the file's bytes with the drawing, and its unmodelled entities as read.
fn attach_source(d: &mut Drawing, bytes: &[u8]) {
    let kept = std::iter::once(&d.model)
        .chain(d.layouts.iter().map(|l| &l.entities))
        .chain(d.blocks.values().map(|b| &b.entities))
        .flat_map(|s| s.iter())
        .filter(|e| matches!(e.kind, cadcraft_doc::EntityKind::Unknown(_)))
        .map(|e| (e.handle, (**e).clone()))
        .collect();
    let mut foreign = (*d.foreign).clone();
    foreign.dwg_source =
        Some(cadcraft_doc::foreign::DwgSource { bytes: std::sync::Arc::from(bytes), kept: std::sync::Arc::new(kept) });
    d.foreign = std::sync::Arc::new(foreign);
}

fn check_size(bytes: &[u8], limits: Limits) -> Result<(), String> {
    if bytes.len() > limits.max_dwg_bytes {
        return Err(format!("DWG: the file is {}, larger than the {} CADCraft opens", mb(bytes.len() as u64), mb(limits.max_dwg_bytes as u64)));
    }
    Ok(())
}

/// Write a drawing as a DWG of `version` (a code from [`VERSIONS`], e.g. "AC1032"), with what
/// the DWG leaves out. A drawing read from a DWG is written into that file's document (see the
/// crate documentation), so what CADCraft doesn't model goes back out unchanged.
pub fn write(d: &Drawing, version: &str) -> Result<(Vec<u8>, Unwritten), String> {
    write_with(d, version, &Plain)
}

/// [`write`] with `ext` writing what the program keeps beyond this crate's translation.
pub fn write_with(d: &Drawing, version: &str, ext: &dyn Extension) -> Result<(Vec<u8>, Unwritten), String> {
    let v = writable(version)?;
    guarded("the DWG writer failed on this drawing", || write::write(d, v, ext))?
}

/// The acadrust version for a code from [`VERSIONS`].
fn writable(version: &str) -> Result<acadrust::DxfVersion, String> {
    use acadrust::DxfVersion as V;
    Ok(match version {
        "AC1015" => V::AC1015,
        "AC1018" => V::AC1018,
        "AC1021" => V::AC1021,
        "AC1024" => V::AC1024,
        "AC1027" => V::AC1027,
        "AC1032" => V::AC1032,
        other => return Err(format!("DWG version `{other}` can't be written (CADCraft writes AC1015 to AC1032)")),
    })
}

/// acadrust, for [`Extension`]s.
pub use acadrust;

/// DXF extended data groups (`1001` application name, then its values) as acadrust's, for
/// [`Extension`]s that write a program's own records.
pub fn xdata_from_groups(tags: &[cadcraft_doc::RawTag]) -> acadrust::xdata::ExtendedData {
    conv::xdata_from_tags(tags)
}

/// Extended data as DXF groups (`1001` application name, then its values), for [`Extension`]s
/// that read the extended data of a program's own records.
pub fn xdata_groups(x: &acadrust::xdata::ExtendedData) -> Vec<cadcraft_doc::RawTag> {
    conv::xdata_tags(x)
}

/// The objects of the DWG a drawing was read from that CADCraft keeps without modelling them
/// (dictionaries, other programs' objects, dynamic block data, xdata), with the entities that
/// carry or use them (block references, viewports, images and the kinds read as unmodelled), as
/// DXF: what a DXF reader keeps of a file, derived from the DWG only when the drawing is saved as
/// DXF (reading and writing DWG never go through DXF). The rest of the entities are left out, so
/// it stays small.
pub fn kept_dxf(source: &[u8]) -> Result<Vec<u8>, String> {
    let (doc, _) = guarded("the DWG reader failed on this file", || read::decode(source, Limits::DEFAULT))??;
    guarded("the DWG reader failed on this file", || kept_of(doc))?
}

fn kept_of(mut doc: acadrust::CadDocument) -> Result<Vec<u8>, String> {
    let names = entity_read::Names::of(&doc);
    let keep = |e: &acadrust::EntityType| {
        use acadrust::EntityType as T;
        matches!(e, T::Insert(_) | T::Viewport(_) | T::RasterImage(_) | T::Block(_) | T::BlockEnd(_) | T::MultiLeader(_) | T::Helix(_))
            || entity_read::entity(e, &names).is_some_and(|x| matches!(x.kind, cadcraft_doc::EntityKind::Unknown(_)))
            // An entity's extension dictionary and reactors are kept as the DXF reader keeps them.
            || e.common().xdictionary_handle.is_some_and(|x| !x.is_null())
            || !e.common().reactors.is_empty()
    };
    let drop: Vec<acadrust::types::Handle> = doc.entities().filter(|e| !keep(e)).map(|e| e.common().handle).collect();
    let gone: std::collections::HashSet<u64> = drop.iter().map(|x| x.value()).collect();
    for x in drop {
        doc.remove_entity_arc(x);
    }
    for b in doc.block_records.iter_mut() {
        b.entity_handles.retain(|x| !gone.contains(&x.value()));
    }
    let mut out = Vec::new();
    acadrust::DxfWriter::new(&doc).write_to_writer(&mut out).map_err(|e| format!("DWG: {e}"))?;
    Ok(out)
}
