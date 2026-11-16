//! Tables (ACAD_TABLE), both ways: their grid, cell texts, merges and the cells' own text
//! heights, and the style they follow. What CADCraft adds of its own (title and header flags,
//! row and cell styles, formulas) is CADCraft's extended data, read and written by its file layer.

use acadrust::entities::table::{self as at, CellRange, CellStylePropertyFlags};
use cadcraft_doc::{Table, TableCell};
use cadcraft_geom::Vec3;

use crate::conv::{a3, v3};

/// Most cells of one table read.
pub const MAX_CELLS: usize = 1_000_000;

fn size(v: f64, d: f64) -> f64 {
    if v.is_finite() && v > 0.0 { v } else { d }
}

/// The cell's own text height, before the table's scale: its first content's when that says it
/// sets one (format override bit 128), else its style's when that says it sets the height;
/// `None` when the cell's height is its cell style's.
fn own_height(c: &at::TableCell) -> Option<f64> {
    let ok = |h: f64| Some(h).filter(|h| h.is_finite() && *h > 0.0 && *h < 1e9);
    if let Some(content) = c.contents.first()
        && content.format_override_flags & 128 != 0
    {
        return ok(content.text_height);
    }
    c.style.as_ref().filter(|s| s.sets(CellStylePropertyFlags::TEXT_HEIGHT) || s.override_flags & 0x20 != 0).and_then(|s| ok(s.text_height))
}

fn text_of(c: &at::TableCell) -> String {
    let Some(content) = c.contents.first() else { return String::new() };
    if !content.value.text.is_empty() { content.value.text.clone() } else { content.value.formatted_value.clone() }
}

/// A table as CADCraft's. `style` is its table style's name and text height. A table another
/// program wrote has no title flag of its own: a first row merged across every column reads as a
/// title (CADCraft's extended data, when there is any, says otherwise).
pub fn read(t: &at::Table, style: (String, f64)) -> Table {
    let mut rows = t.rows.len();
    let mut cols = t.columns.len().max(t.rows.iter().map(|r| r.cells.len()).max().unwrap_or(0));
    if rows.saturating_mul(cols) > MAX_CELLS || rows == 0 || cols == 0 {
        rows = 0;
        cols = 0;
    }
    let row_heights = (0..rows).map(|r| size(t.rows.get(r).map_or(0.0, |x| x.height), 0.27)).collect();
    let col_widths = (0..cols).map(|c| size(t.columns.get(c).map_or(0.0, |x| x.width), 2.5)).collect();
    let mut cells: Vec<Vec<TableCell>> = (0..rows)
        .map(|r| {
            (0..cols)
                .map(|c| match t.rows.get(r).and_then(|x| x.cells.get(c)) {
                    Some(cell) => TableCell { text: text_of(cell), text_height: own_height(cell), ..TableCell::default() },
                    None => TableCell::default(),
                })
                .collect()
        })
        .collect();
    // Merges: each range's first cell spans it.
    let mut spans: Vec<(usize, usize, u32, u32)> = t
        .merged_ranges
        .iter()
        .map(|m| (m.top_row, m.left_col, u32::try_from(m.row_count()).unwrap_or(1), u32::try_from(m.col_count()).unwrap_or(1)))
        .collect();
    for (r, row) in t.rows.iter().enumerate() {
        for (c, cell) in row.cells.iter().enumerate() {
            if cell.merge_width >= 1 && cell.merge_height >= 1 && (cell.merge_width, cell.merge_height) != (1, 1) {
                spans.push((r, c, u32::try_from(cell.merge_height).unwrap_or(1), u32::try_from(cell.merge_width).unwrap_or(1)));
            }
        }
    }
    for (r, c, rs, cs) in spans {
        if let Some(cell) = cells.get_mut(r).and_then(|x| x.get_mut(c))
            && (rs, cs) != (1, 1)
            && cell.merged.is_none()
        {
            cell.merged = Some((rs, cs));
        }
    }
    let title = cols > 1 && cells.first().and_then(|r| r.first()).and_then(|c| c.merged) == Some((1, cols as u32));
    Table {
        insert: v3(t.insertion_point),
        col_widths,
        row_heights,
        cells,
        style: style.0,
        text_height: style.1,
        title,
        header: false,
        ..Table::default()
    }
}

/// Write CADCraft's table onto acadrust's (`x` the file's table, or a new one): its grid, texts,
/// merges and cells' own heights. Cells whose text, merge and height are what the file's cell
/// says keep the file's cell as it was.
pub fn put(x: &mut at::Table, t: &Table) {
    x.insertion_point = a3(t.insert);
    if x.horizontal_direction.x == 0.0 && x.horizontal_direction.y == 0.0 && x.horizontal_direction.z == 0.0 {
        x.horizontal_direction = a3(Vec3::new(1.0, 0.0, 0.0));
    }
    let cols = t.col_widths.len();
    x.columns.truncate(cols);
    while x.columns.len() < cols {
        x.columns.push(at::TableColumn::new());
    }
    for (c, w) in x.columns.iter_mut().zip(&t.col_widths) {
        c.width = *w;
    }
    let rows = t.row_heights.len();
    x.rows.truncate(rows);
    while x.rows.len() < rows {
        x.rows.push(at::TableRow::new(cols));
    }
    for (r, (row, h)) in x.rows.iter_mut().zip(&t.row_heights).enumerate() {
        row.height = *h;
        row.cells.truncate(cols);
        while row.cells.len() < cols {
            row.cells.push(at::TableCell::new());
        }
        for (c, cell) in row.cells.iter_mut().enumerate() {
            let want = t.cells.get(r).and_then(|x| x.get(c)).cloned().unwrap_or_default();
            let same = text_of(cell) == want.text && own_height(cell) == want.text_height;
            if !same {
                if text_of(cell) != want.text {
                    cell.set_text(&want.text);
                }
                put_height(cell, want.text_height);
            }
            cell.merge_width = 1;
            cell.merge_height = 1;
            cell.merged = 0;
        }
    }
    x.merged_ranges.clear();
    for (r, row) in t.cells.iter().enumerate() {
        for (c, cell) in row.iter().enumerate() {
            if let Some((rs, cs)) = cell.merged.filter(|m| *m != (1, 1)) {
                let (rs, cs) = (rs.max(1) as usize, cs.max(1) as usize);
                let range = CellRange::new(r, c, (r + rs - 1).min(rows.saturating_sub(1)), (c + cs - 1).min(cols.saturating_sub(1)));
                x.merge_cells(range);
            }
        }
    }
}

/// A cell's own text height (`None`: its cell style's).
fn put_height(cell: &mut at::TableCell, h: Option<f64>) {
    match h {
        Some(h) => {
            if cell.contents.is_empty() {
                cell.contents.push(at::CellContent::new());
            }
            if let Some(c) = cell.contents.first_mut() {
                c.format_override_flags |= 128;
                c.text_height = h;
            }
        }
        None => {
            for c in &mut cell.contents {
                c.format_override_flags &= !128;
            }
            if let Some(s) = cell.style.as_mut() {
                s.property_flags.remove(CellStylePropertyFlags::TEXT_HEIGHT);
                s.override_flags &= !0x20;
            }
        }
    }
}
