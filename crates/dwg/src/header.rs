//! Header variables both ways, under the names CADCraft's header uses (DXF `$` names without the
//! `$`), with the values as the DXF reader takes them (angles in radians but `DIMJOGANG` in
//! degrees, `DIMDSEP` as its character, colours as ACI numbers).

use acadrust::document::HeaderVariables;
use acadrust::types::{Color as AColor, Vector2, Vector3};
use cadcraft_doc::{HVal, Header};
use cadcraft_geom::Vec3;

fn p3(v: Vector3) -> HVal {
    HVal::Point(Vec3::new(v.x, v.y, v.z))
}

fn p2(v: Vector2) -> HVal {
    HVal::Point(Vec3::new(v.x, v.y, 0.0))
}

fn aci(c: &AColor) -> HVal {
    HVal::Int(i64::from(c.approximate_index()))
}

/// The variables the table below lists, read and written the same way.
macro_rules! vars {
    ($h:ident, $mode:ident, $hd:ident) => {
        vars!(@ $h, $mode, $hd,
            bool: ORTHOMODE ortho_mode, REGENMODE regen_mode, FILLMODE fill_mode, QTEXTMODE quick_text_mode, MIRRTEXT mirror_text,
                  DISPSILH display_silhouette, LWDISPLAY lineweight_display, PLINEGEN polyline_linetype_generation,
                  PSLTSCALE paper_space_linetype_scaling, DIMTOL dim_tolerance, DIMLIM dim_limits, DIMTIH dim_text_inside_horizontal,
                  DIMTOH dim_text_outside_horizontal, DIMSE1 dim_suppress_ext1, DIMSE2 dim_suppress_ext2, DIMALT dim_alternate_units,
                  DIMTOFL dim_force_line_inside, DIMTIX dim_force_text_inside, DIMSOXD dim_suppress_outside_ext, DIMSAH dim_separate_arrows,
                  DIMSD1 dim_suppress_line1, DIMSD2 dim_suppress_line2, DIMFXLON dim_fixed_ext_line_on, SPLFRAME spline_frame,
                  REALWORLDSCALE real_world_scale, WORLDVIEW world_view, TILEMODE show_model_space, PLIMCHECK paper_space_limit_check,
                  VISRETAIN retain_xref_visibility, PSTYLEMODE plotstyle_mode;
            int: ATTMODE attribute_visibility, SKPOLY sketch_type, LUNITS linear_unit_format, LUPREC linear_unit_precision,
                 AUNITS angular_unit_format, AUPREC angular_unit_precision, ANGDIR angle_direction, MEASUREMENT measurement,
                 INSUNITS insertion_units, PDMODE point_display_mode, DIMTAD dim_text_above, DIMZIN dim_zero_suppression,
                 DIMALTD dim_alt_decimal_places, DIMLUNIT dim_linear_unit_format, DIMDEC dim_decimal_places,
                 DIMTDEC dim_tolerance_decimal_places, DIMALTU dim_alt_units_format, DIMALTTD dim_alt_tolerance_decimal_places,
                 DIMAUNIT dim_angular_units, DIMADEC dim_angular_decimal_places, DIMJUST dim_horizontal_justification,
                 DIMTOLJ dim_tolerance_justification, DIMTZIN dim_tolerance_zero_suppression, DIMALTZ dim_alt_tolerance_zero_suppression,
                 DIMALTTZ dim_alt_tolerance_zero_tight, DIMATFIT dim_fit, DIMTMOVE dim_text_movement, DIMFRAC dim_fraction_format,
                 DIMLWD dim_line_weight, DIMLWE dim_ext_line_weight, DIMARCSYM dim_arc_symbol, DIMTFILL dim_text_fill,
                 SPLINETYPE spline_type, SPLINESEGS spline_segments, SURFTAB1 surface_tab1, SURFTAB2 surface_tab2, SURFTYPE surface_type,
                 SURFU surface_u_density, SURFV surface_v_density, MAXACTVP max_active_viewports, UNITMODE unit_mode,
                 XCLIPFRAME xclip_frame, DWFFRAME dwf_frame, DGNFRAME dgn_frame, SHOWHIST show_solid_history, CELWEIGHT current_line_weight;
            real: LTSCALE linetype_scale, TEXTSIZE text_height, TRACEWID trace_width, ELEVATION elevation, THICKNESS thickness,
                  FILLETRAD fillet_radius, CHAMFERA chamfer_distance_a, CHAMFERB chamfer_distance_b, SKETCHINC sketch_increment,
                  PDSIZE point_display_size, CELTSCALE current_entity_linetype_scale, DIMSCALE dim_scale, DIMASZ dim_arrow_size,
                  DIMEXO dim_ext_line_offset, DIMDLI dim_line_increment, DIMRND dim_rounding, DIMDLE dim_line_extension,
                  DIMEXE dim_ext_line_extension, DIMTP dim_tolerance_plus, DIMTM dim_tolerance_minus, DIMTXT dim_text_height,
                  DIMCEN dim_center_mark, DIMTSZ dim_tick_size, DIMGAP dim_line_gap, DIMALTF dim_alt_scale, DIMLFAC dim_linear_scale,
                  DIMTVP dim_text_vertical_pos, DIMTFAC dim_tolerance_scale, DIMFXL dim_fixed_ext_line_length, DIMALTRND dim_alt_rounding,
                  PSOLWIDTH polysolid_width, PSOLHEIGHT polysolid_height, PELEVATION paper_elevation, PLINEWID polyline_width,
                  TDCREATE create_date_julian, TDUPDATE update_date_julian, TDINDWG total_editing_time, TDUCREATE universal_create_date_julian,
                  TDUUPDATE universal_update_date_julian;
            string: DWGCODEPAGE code_page, TEXTSTYLE current_text_style_name, CMLSTYLE multiline_style, CTABLESTYLE current_table_style_name,
                    CMLEADERSTYLE current_mleader_style_name, CLAYER current_layer_name, CELTYPE current_linetype_name,
                    DIMPOST dim_post, DIMAPOST dim_alt_post, DIMTXSTY dim_text_style_name, DIMLTYPE dim_linetype_name,
                    DIMLTEX1 dim_linetype1_name, DIMLTEX2 dim_linetype2_name, DIMSTYLE current_dimstyle_name,
                    FINGERPRINTGUID fingerprint_guid, VERSIONGUID version_guid, LASTSAVEDBY last_saved_by, HYPERLINKBASE hyperlink_base,
                    CANNOSCALE current_annotation_scale, DIMBLK dim_arrow_block, DIMBLK1 dim_arrow_block1, DIMBLK2 dim_arrow_block2,
                    DIMLDRBLK dim_leader_arrow_block;
            point3: INSBASE model_space_insertion_base, EXTMIN model_space_extents_min, EXTMAX model_space_extents_max,
                    UCSORG model_space_ucs_origin, UCSXDIR model_space_ucs_x_axis, UCSYDIR model_space_ucs_y_axis;
            point2: LIMMIN model_space_limits_min, LIMMAX model_space_limits_max;
            color: CECOLOR current_entity_color, DIMCLRD dim_line_color, DIMCLRE dim_ext_line_color, DIMCLRT dim_text_color,
                   DIMTFILLCLR dim_text_fill_color, INTERFERECOLOR interference_color
        )
    };
    (@ $h:ident, read, $hd:ident,
        bool: $($bn:tt $bf:ident),*; int: $($in:tt $if_:ident),*; real: $($rn:tt $rf:ident),*; string: $($sn:tt $sf:ident),*;
        point3: $($pn:tt $pf:ident),*; point2: $($qn:tt $qf:ident),*; color: $($cn:tt $cf:ident),*) => {
        $( $h.set(stringify!($bn), HVal::Int(i64::from($hd.$bf))); )*
        $( $h.set(stringify!($in), HVal::Int(i64::from($hd.$if_))); )*
        $( $h.set(stringify!($rn), HVal::Real(f64::from($hd.$rf))); )*
        $( $h.set(stringify!($sn), HVal::Str($hd.$sf.clone())); )*
        $( $h.set(stringify!($pn), p3($hd.$pf)); )*
        $( $h.set(stringify!($qn), p2($hd.$qf)); )*
        $( $h.set(stringify!($cn), aci(&$hd.$cf)); )*
    };
    (@ $h:ident, write, $hd:ident,
        bool: $($bn:tt $bf:ident),*; int: $($in:tt $if_:ident),*; real: $($rn:tt $rf:ident),*; string: $($sn:tt $sf:ident),*;
        point3: $($pn:tt $pf:ident),*; point2: $($qn:tt $qf:ident),*; color: $($cn:tt $cf:ident),*) => {
        $( if let Some(v) = $h.get(stringify!($bn)).and_then(HVal::as_i64) { $hd.$bf = v != 0; } )*
        $( if let Some(v) = $h.get(stringify!($in)).and_then(HVal::as_i64).and_then(|v| v.try_into().ok()) { $hd.$if_ = v; } )*
        $( if let Some(v) = $h.get(stringify!($rn)).and_then(HVal::as_f64).filter(|v| v.is_finite()) { $hd.$rf = v as _; } )*
        $( if let Some(v) = $h.get(stringify!($sn)).and_then(HVal::as_str) { $hd.$sf = v.to_string(); } )*
        $( if let Some(HVal::Point(p)) = $h.get(stringify!($pn)) { $hd.$pf = Vector3::new(p.x, p.y, p.z); } )*
        $( if let Some(HVal::Point(p)) = $h.get(stringify!($qn)) { $hd.$qf = Vector2::new(p.x, p.y); } )*
        $( if let Some(v) = $h.get(stringify!($cn)).and_then(HVal::as_i64).and_then(|v| i16::try_from(v).ok()) {
            if $hd.$cf.approximate_index() != v { $hd.$cf = AColor::from_index(v); }
        } )*
    };
}

/// The header of a drawing read from `hd` of a file of `version` (`$ACADVER`, e.g. "AC1024").
pub fn read(hd: &HeaderVariables, version: &str) -> Header {
    let mut h = Header::default();
    h.set("ACADVER", HVal::Str(version.to_string()));
    h.set("ACADMAINTVER", HVal::Int(0));
    h.set("HANDSEED", HVal::Str(format!("{:X}", hd.handle_seed)));
    vars!(h, read, hd);
    h.set("ANGBASE", HVal::Real(hd.angle_base));
    h.set("OSMODE", HVal::Int(i64::from(hd.object_snap_mode)));
    h.set("DIMJOGANG", HVal::Real((hd.dim_jog_angle.to_degrees() * 1e9).round() / 1e9));
    if !hd.dim_decimal_separator.is_control() {
        h.set("DIMDSEP", HVal::Str(hd.dim_decimal_separator.to_string()));
    }
    h.set("LIGHTGLYPHDISPLAY", HVal::Int(i64::from(hd.light_glyph_display)));
    h.set("TILEMODELIGHTSYNCH", HVal::Int(i64::from(hd.tile_model_light_synch)));
    h.set("SOLIDHIST", HVal::Int(i64::from(hd.record_solid_history)));
    for name in ["DIMBLK", "DIMBLK1", "DIMBLK2", "DIMLDRBLK", "HYPERLINKBASE", "CANNOSCALE"] {
        if h.get(name).and_then(HVal::as_str).is_some_and(str::is_empty) {
            h.vars.remove(name);
        }
    }
    h
}

/// Write the drawing's header variables onto acadrust's. Variables the drawing doesn't carry stay.
pub fn write(h: &Header, hd: &mut HeaderVariables) {
    vars!(h, write, hd);
    if let Some(v) = h.get("ANGBASE").and_then(HVal::as_f64).filter(|v| v.is_finite()) {
        hd.angle_base = v;
    }
    if let Some(v) = h.get("OSMODE").and_then(HVal::as_i64).and_then(|v| i32::try_from(v).ok()) {
        hd.object_snap_mode = v;
    }
    if let Some(v) = h.get("DIMJOGANG").and_then(HVal::as_f64).filter(|v| v.is_finite()) {
        hd.dim_jog_angle = v.to_radians();
    }
    if let Some(c) = h.get("DIMDSEP").and_then(HVal::as_str).and_then(|s| s.chars().next()) {
        hd.dim_decimal_separator = c;
    }
}
