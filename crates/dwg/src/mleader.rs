//! Multileaders written from CADCraft's (the reader is `entity_read::mleader`): one leader root
//! at the landing holding every leader line, the dogleg towards the content, and MText or block
//! content, with the entity's own values for what CADCraft models (leader type, landing and
//! dogleg, arrowhead size, content type, text style and height, block content).

use acadrust::entities::{self as ae};
use acadrust::types::{Handle as AHandle, Vector3};
use cadcraft_doc::{Drawing, EntityKind, MLeader};
use cadcraft_geom::Vec3;

use crate::conv::{a3, ah};

/// What a multileader refers to by handle: its style, its text style and its block.
pub struct Refs {
    pub style: Option<AHandle>,
    pub text_style: Option<AHandle>,
    pub block: Option<AHandle>,
}

fn attachment(v: i16) -> ae::TextAttachmentType {
    match v {
        2 => ae::TextAttachmentType::MiddleOfText,
        4 => ae::TextAttachmentType::BottomOfBottomLine,
        _ => ae::TextAttachmentType::TopOfTopLine,
    }
}

/// Write CADCraft's multileader onto acadrust's (`x` the file's multileader, or a new one).
pub fn put(x: &mut ae::MultiLeader, m: &MLeader, d: &Drawing, r: &Refs) {
    let style = d.mleader_styles.iter().find(|s| s.name.eq_ignore_ascii_case(&m.style)).cloned().unwrap_or_default();
    let block = m.block.as_ref().zip(r.block);
    let text = if block.is_some() { None } else { m.text.as_ref() };
    let content_pt = block.map(|(b, _)| b.insert).or(text.map(|t| t.insert));
    let dir = if content_pt.is_some_and(|c| c.x < m.landing.x) { -1.0 } else { 1.0 };
    let attach = text.map_or(1, |t| t.attach.clamp(1, 9));
    let vert: i16 = match attach {
        1..=3 => 0,
        4..=6 => 2,
        _ => 4,
    };
    let align = (attach - 1) % 3;
    let align_t = match align {
        1 => ae::TextAlignmentType::Center,
        2 => ae::TextAlignmentType::Right,
        _ => ae::TextAlignmentType::Left,
    };
    let attach_point = match align {
        1 => ae::TextAttachmentPointType::Center,
        2 => ae::TextAttachmentPointType::Right,
        _ => ae::TextAttachmentPointType::Left,
    };

    let cx = &mut x.context;
    cx.scale_factor = 1.0;
    cx.content_base_point = a3(m.landing + Vec3::new(m.dogleg * dir, 0.0, 0.0));
    cx.text_height = text.map_or(style.text_height, |t| t.height);
    cx.arrowhead_size = m.arrow_size;
    cx.landing_gap = style.landing_gap;
    cx.text_left_attachment = attachment(vert);
    cx.text_right_attachment = attachment(vert);
    cx.text_alignment = align_t;
    cx.text_attachment_point = attach_point;
    cx.has_text_contents = text.is_some();
    match text {
        Some(t) => {
            cx.text_string.clone_from(&t.contents);
            cx.text_normal = Vector3::new(0.0, 0.0, 1.0);
            cx.text_style_handle = r.text_style;
            cx.text_location = a3(t.insert);
            cx.text_direction = Vector3::new(t.rotation.cos(), t.rotation.sin(), 0.0);
            cx.text_rotation = t.rotation;
            cx.text_width = t.width;
            cx.text_boundary_height = 0.0;
            cx.line_spacing_factor = t.line_spacing;
            cx.line_spacing_style = if t.line_spacing_exact { ae::LineSpacingStyle::Exactly } else { ae::LineSpacingStyle::AtLeast };
            cx.text_color = acadrust::types::Color::ByBlock;
        }
        None => cx.text_string.clear(),
    }
    cx.has_block_contents = block.is_some();
    match block {
        Some((b, h)) => {
            cx.block_content_handle = Some(h);
            cx.block_content_normal = Vector3::new(0.0, 0.0, 1.0);
            cx.block_content_location = a3(b.insert);
            cx.block_content_scale = a3(b.scale);
            cx.block_rotation = b.rotation;
            cx.block_content_color = acadrust::types::Color::ByBlock;
            // Block → world, row by row.
            let base = d.block(&b.block).map_or(Vec3::ZERO, |x| x.base);
            let mx = b.transform(base.xy());
            let z = b.insert.z - b.scale.z * base.z;
            cx.transform_matrix = [mx.a, mx.c, 0.0, mx.e, mx.b, mx.d, 0.0, mx.f, 0.0, 0.0, b.scale.z, z, 0.0, 0.0, 0.0, 1.0];
        }
        None => cx.block_content_handle = None,
    }
    cx.base_point = Vector3::new(0.0, 0.0, m.landing.z);
    cx.base_direction = Vector3::new(1.0, 0.0, 0.0);
    cx.base_vertical = Vector3::new(0.0, 1.0, 0.0);
    cx.normal_reversed = false;
    cx.leader_roots.clear();
    if !m.leaders.is_empty() {
        let mut root = ae::LeaderRoot::new(0);
        root.content_valid = true;
        root.unknown = true;
        root.connection_point = a3(m.landing);
        root.direction = Vector3::new(dir, 0.0, 0.0);
        root.landing_distance = m.dogleg;
        for (n, l) in m.leaders.iter().enumerate() {
            let mut line = ae::LeaderLine::new(i32::try_from(n).unwrap_or(i32::MAX));
            line.points = l.iter().map(|p| a3(*p)).collect();
            root.lines.push(line);
        }
        cx.leader_roots.push(root);
    }

    x.style_handle = r.style;
    x.path_type = if m.leaders.is_empty() {
        ae::MultiLeaderPathType::Invisible
    } else if m.spline {
        ae::MultiLeaderPathType::Spline
    } else {
        ae::MultiLeaderPathType::StraightLineSegments
    };
    x.line_color = acadrust::types::Color::ByBlock;
    x.enable_landing = true;
    x.enable_dogleg = m.dogleg > 0.0;
    x.dogleg_length = m.dogleg;
    x.arrowhead_size = m.arrow_size;
    x.content_type = if block.is_some() {
        ae::LeaderContentType::Block
    } else if text.is_some() {
        ae::LeaderContentType::MText
    } else {
        ae::LeaderContentType::None
    };
    x.text_style_handle = r.text_style;
    x.text_left_attachment = attachment(vert);
    x.text_right_attachment = attachment(vert);
    x.text_attachment_point = attach_point;
    x.text_alignment = align_t;
    x.text_color = acadrust::types::Color::ByBlock;
    x.text_height = cx_height(m, &style);
    x.block_content_handle = block.map(|(_, h)| h);
    x.block_content_color = acadrust::types::Color::ByBlock;
    if let Some((b, _)) = block {
        x.block_scale = a3(b.scale);
        x.block_rotation = b.rotation;
        x.block_connection_type = ae::BlockContentConnectionType::BasePoint;
        // Attribute values by their ATTDEF in the block.
        let defs: Vec<(cadcraft_doc::Handle, &str)> = d
            .block(&b.block)
            .map(|blk| {
                blk.entities.iter().filter_map(|e| if let EntityKind::AttDef(ad) = &e.kind { Some((e.handle, ad.tag.as_str())) } else { None }).collect()
            })
            .unwrap_or_default();
        x.block_attributes = b
            .attribs
            .iter()
            .filter_map(|a| defs.iter().find(|(_, tag)| tag.eq_ignore_ascii_case(&a.tag)).map(|(h, _)| (*h, a)))
            .enumerate()
            .map(|(n, (h, a))| {
                let mut ba = ae::BlockAttribute::new(&a.text.value);
                ba.attribute_definition_handle = Some(ah(h));
                ba.index = i16::try_from(n).unwrap_or(i16::MAX);
                ba
            })
            .collect();
    } else {
        x.block_attributes.clear();
    }
    use ae::MultiLeaderPropertyOverrideFlags as F;
    x.property_override_flags =
        F::PATH_TYPE | F::ENABLE_LANDING | F::ENABLE_DOGLEG | F::LANDING_DISTANCE | F::ARROWHEAD_SIZE | F::CONTENT_TYPE | F::TEXT_STYLE | F::TEXT_HEIGHT;
}

fn cx_height(m: &MLeader, style: &cadcraft_doc::MLeaderStyle) -> f64 {
    m.text.as_ref().map_or(style.text_height, |t| t.height)
}
