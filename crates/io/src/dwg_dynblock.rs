//! Dynamic block data of a DWG, in the form CADCraft's model keeps it: acadrust reads a dynamic
//! block's evaluation graph, parameters, actions, grips and the references' representation data
//! as typed objects; here each becomes the groups of its record (DXF Reference, "Dynamic block
//! objects"), which `cadcraft_doc::dynblock` reads and edits. Nothing is written or parsed as
//! text: the groups are made from the objects' fields. The objects themselves stay in the DWG the
//! drawing was read from and are written back from there; a reference's representation data the
//! drawing changes goes back through [`crate::dwg_ext`].

use cadcraft_dwg::acadrust::objects::{
    BlockAction, BlockActionOffsets, BlockActionWithBasePoint, BlockConnection, BlockConstraintParameter, BlockElement, BlockEvalExpression,
    BlockEvalValue, BlockGrip, BlockOnePointParameter, BlockParameter, BlockParameterProperty, BlockParameterValueSet, BlockTwoPointParameter,
    BlockVisibilityParameter, DynamicBlockData as D, DynamicBlockObject,
};
use cadcraft_dwg::acadrust::types::{Handle as AHandle, Vector3};
use craft_dxf::Tag;

/// The groups of one record, made in order.
struct G(Vec<Tag>);

impl G {
    fn s(&mut self, code: i32, v: &str) {
        self.0.push(Tag::s(code, v));
    }
    fn i(&mut self, code: i32, v: i64) {
        self.0.push(Tag::i(code, v));
    }
    fn b(&mut self, code: i32, v: bool) {
        self.i(code, i64::from(v));
    }
    fn r(&mut self, code: i32, v: f64) {
        self.0.push(Tag::s(code, format!("{v:?}")));
    }
    fn h(&mut self, code: i32, v: AHandle) {
        self.0.push(Tag::s(code, format!("{:X}", v.value())));
    }
    /// A point: `x`, `x + 10`, `x + 20`.
    fn p(&mut self, x: i32, v: Vector3) {
        self.r(x, v.x);
        self.r(x + 10, v.y);
        self.r(x + 20, v.z);
    }
    fn marker(&mut self, name: &str) {
        self.s(100, name);
    }
    fn len(&mut self, code: i32, n: usize) {
        self.i(code, i64::try_from(n).unwrap_or(i64::MAX));
    }
}

/// `AcDbEvalExpr`: the graph id, its version, and its value when it has one (-9999: none).
fn eval(g: &mut G, node: i32, major: i32, minor: i32, code: i16, value: &BlockEvalValue) {
    g.marker("AcDbEvalExpr");
    g.i(90, i64::from(node));
    g.i(98, i64::from(major));
    g.i(99, i64::from(minor));
    if code == -9999 {
        return;
    }
    g.s(1, "");
    g.i(70, i64::from(code));
    match value {
        BlockEvalValue::Real(v) => g.r(140, *v),
        BlockEvalValue::Point(v) => {
            let x = if matches!(code, 10 | 11) { i32::from(code) } else { 10 };
            g.r(x, v[0]);
            g.r(x + 10, v[1]);
        }
        BlockEvalValue::Text(v) => g.s(1, v),
        BlockEvalValue::Long(v) => g.i(90, i64::from(*v)),
        BlockEvalValue::Handle(v) => g.h(91, *v),
        BlockEvalValue::Short(v) => g.i(70, i64::from(*v)),
        BlockEvalValue::None => {}
    }
}

fn expr(g: &mut G, e: &BlockEvalExpression) {
    eval(g, e.node_id, e.major, e.minor, e.value_code, &e.value);
}

/// `AcDbBlockElement`: the name the block editor shows.
fn element(g: &mut G, e: &BlockElement) {
    expr(g, &e.eval);
    g.marker("AcDbBlockElement");
    g.s(300, &e.name);
    g.i(98, i64::from(e.major));
    g.i(99, i64::from(e.minor));
    g.i(1071, i64::from(e.eed_1071));
}

fn parameter(g: &mut G, p: &BlockParameter) {
    element(g, &p.element);
    g.marker("AcDbBlockParameter");
    g.b(280, p.show_properties);
    g.b(281, p.chain_actions);
}

/// A property's connections: their count (`count`), then each one's parameter id and name.
fn property(g: &mut G, p: &BlockParameterProperty, count: i32, id: i32, name: i32) {
    g.len(count, p.connections.len());
    for c in &p.connections {
        g.i(id, i64::from(c.code));
        g.s(name, &c.name);
    }
}

fn two_point(g: &mut G, p: &BlockTwoPointParameter) {
    parameter(g, &p.parameter);
    g.marker("AcDbBlock2PtParameter");
    g.p(1010, p.definition_base_point);
    g.p(1011, p.definition_end_point);
    g.i(170, 4);
    for s in p.property_states {
        g.i(91, i64::from(s));
    }
    for (k, prop) in (0i32..).zip(&p.properties) {
        property(g, prop, 171 + k, 92 + k, 301 + k);
    }
    g.i(177, i64::from(p.parameter_base_location));
}

fn one_point(g: &mut G, p: &BlockOnePointParameter) {
    parameter(g, &p.parameter);
    g.marker("AcDbBlock1PtParameter");
    g.p(1010, p.definition_point);
    g.i(93, i64::from(p.property_count));
    for (k, prop) in (0i32..).zip(&p.properties) {
        property(g, prop, 170 + k, 91 + k, 301 + k);
    }
}

fn constraint(g: &mut G, c: &BlockConstraintParameter) {
    two_point(g, &c.parameter);
    g.marker("AcDbBlockConstraintParameter");
    g.h(330, c.dependency);
}

/// A value set: its description, flags, minimum, maximum and increment (`real`, `+1`, `+2`), and
/// the listed values (`count`, each at `real + 3`).
fn value_set(g: &mut G, v: &BlockParameterValueSet, (flags, real, count, description): (i32, i32, i32, i32)) {
    g.s(description, &v.description);
    g.i(flags, i64::from(v.flags));
    g.r(real, v.minimum);
    g.r(real + 1, v.maximum);
    g.r(real + 2, v.increment);
    g.len(count, v.values.len());
    for x in &v.values {
        g.r(real + 3, *x);
    }
}

fn grip(g: &mut G, x: &BlockGrip) {
    element(g, &x.element);
    g.marker("AcDbBlockGrip");
    g.i(91, i64::from(x.flags_91));
    g.i(92, i64::from(x.flags_92));
    g.p(1010, x.location);
    g.b(280, x.insert_cycling);
    g.i(93, i64::from(x.insert_cycling_weight));
}

fn orientation(g: &mut G, v: Vector3) {
    g.r(140, v.x);
    g.r(141, v.y);
    g.r(142, v.z);
}

fn action(g: &mut G, a: &BlockAction) {
    element(g, &a.element);
    g.marker("AcDbBlockAction");
    g.len(70, a.action_ids.len());
    for id in &a.action_ids {
        g.i(91, i64::from(*id));
    }
    g.len(71, a.dependencies.len());
    for h in &a.dependencies {
        g.h(330, *h);
    }
    g.p(1010, a.display_location);
}

/// Connections: their ids from `id` on, then their names from `name` on.
fn connections(g: &mut G, list: &[BlockConnection], id: i32, name: i32) {
    for (k, c) in (0i32..).zip(list) {
        g.i(id + k, i64::from(c.code));
    }
    for (k, c) in (0i32..).zip(list) {
        g.s(name + k, &c.name);
    }
}

fn offsets(g: &mut G, o: &BlockActionOffsets) {
    g.r(140, o.distance_multiplier);
    g.r(141, o.angle_offset);
    g.i(280, i64::from(o.flags));
}

fn with_base(g: &mut G, a: &BlockActionWithBasePoint) {
    action(g, &a.action);
    g.marker("AcDbBlockActionWithBasePt");
    connections(g, &a.connections, 92, 301);
    g.p(1011, a.offset);
    g.b(280, a.dependent);
    g.p(1012, a.base_point);
}

fn head(g: &mut G, handle: AHandle, owner: AHandle, reactors: &[AHandle], xdict: Option<AHandle>) {
    g.h(5, handle);
    if !reactors.is_empty() {
        g.s(102, "{ACAD_REACTORS");
        for r in reactors {
            g.h(330, *r);
        }
        g.s(102, "}");
    }
    if let Some(x) = xdict.filter(|x| !x.is_null()) {
        g.s(102, "{ACAD_XDICTIONARY");
        g.h(360, x);
        g.s(102, "}");
    }
    g.h(330, owner);
}

/// A visibility parameter: its states and the objects each shows.
pub(crate) fn visibility(v: &BlockVisibilityParameter) -> (String, Vec<Tag>) {
    let mut g = G(Vec::new());
    head(&mut g, v.handle, v.owner, &[], None);
    eval(&mut g, v.eval_node_id, v.eval_major, v.eval_minor, v.eval_value_code, &v.eval_value);
    g.marker("AcDbBlockElement");
    g.s(300, &v.element_name);
    g.i(98, i64::from(v.element_major));
    g.i(99, i64::from(v.element_minor));
    g.i(1071, i64::from(v.element_eed_1071));
    g.marker("AcDbBlockParameter");
    g.b(280, v.show_properties);
    g.b(281, v.chain_actions);
    g.marker("AcDbBlock1PtParameter");
    g.p(1010, v.def_point);
    g.i(93, i64::from(v.property_info_count));
    for (k, p) in (0i32..).zip(&v.property_info) {
        g.len(170 + k, p.connections.len());
        for c in &p.connections {
            g.i(91 + k, i64::from(c.code));
            g.s(301 + k, &c.name);
        }
    }
    g.marker("AcDbBlockVisibilityParameter");
    g.b(281, v.is_initialized);
    g.s(301, &v.name);
    g.s(302, &v.description);
    g.b(91, v.unknown_bool);
    g.len(93, v.all_blocks.len());
    for h in &v.all_blocks {
        g.h(331, *h);
    }
    g.len(92, v.states.len());
    for s in &v.states {
        g.s(303, &s.name);
        g.len(94, s.visible_blocks.len());
        for h in &s.visible_blocks {
            g.h(332, *h);
        }
        g.len(95, s.visible_params.len());
        for h in &s.visible_params {
            g.h(333, *h);
        }
    }
    ("BLOCKVISIBILITYPARAMETER".into(), g.0)
}

/// The record of a dynamic block object: its type and groups, `None` for one that is not part of
/// a dynamic block (solid history, which acadrust reads among these).
pub(crate) fn object(o: &DynamicBlockObject) -> Option<(String, Vec<Tag>)> {
    if let D::VisibilityParameter(v) = &o.data {
        return Some(visibility(v));
    }
    if matches!(o.data, D::SolidHistory(_) | D::SolidHistoryNode(_)) {
        return None;
    }
    let mut g = G(Vec::new());
    head(&mut g, o.handle, o.owner, &o.reactors, o.xdictionary_handle);
    let own = o.cpp_class_name.as_str();
    match &o.data {
        D::Unknown | D::SolidHistory(_) | D::SolidHistoryNode(_) | D::VisibilityParameter(_) => {}
        D::Representation(r) => {
            g.marker(own);
            g.i(70, i64::from(r.flags));
            g.h(340, r.block);
        }
        D::ProxyNode(e) => {
            expr(&mut g, e);
            g.marker("AcDbDynamicBlockProxyNode");
        }
        D::GripLocationComponent(e) => {
            expr(&mut g, &e.eval);
            g.marker("AcDbBlockGripExpr");
            g.i(91, i64::from(e.grip_type));
            g.s(300, &e.expression);
        }
        D::AlignmentGrip(x) | D::LinearGrip(x) => {
            grip(&mut g, &x.grip);
            g.marker(if matches!(o.data, D::AlignmentGrip(_)) { "AcDbBlockAlignmentGrip" } else { "AcDbBlockLinearGrip" });
            orientation(&mut g, x.orientation);
        }
        D::FlipGrip(x) => {
            grip(&mut g, &x.grip);
            g.marker("AcDbBlockFlipGrip");
            orientation(&mut g, x.orientation);
            g.i(93, i64::from(x.combined_state));
        }
        D::LookupGrip(x) | D::PolarGrip(x) | D::RotationGrip(x) | D::VisibilityGrip(x) | D::XYGrip(x) | D::PropertiesTableGrip(x) => {
            grip(&mut g, x);
            g.marker(own);
        }
        D::AlignmentParameter(x) => {
            two_point(&mut g, &x.parameter);
            g.marker("AcDbBlockAlignmentParameter");
            g.b(280, x.align_perpendicular);
        }
        D::BasePointParameter(x) => {
            one_point(&mut g, &x.parameter);
            g.marker("AcDbBlockBasepointParameter");
            g.p(1011, x.point);
            g.p(1012, x.base_point);
        }
        D::FlipParameter(x) => {
            two_point(&mut g, &x.parameter);
            g.marker("AcDbBlockFlipParameter");
            g.s(305, &x.flip_label);
            g.s(306, &x.flip_label_description);
            g.s(307, &x.base_state_label);
            g.s(308, &x.flipped_state_label);
            g.p(1012, x.definition_label_point);
            g.s(309, &x.tooltip);
            g.i(96, i64::from(x.flags_96));
        }
        D::LinearParameter(x) => {
            two_point(&mut g, &x.parameter);
            g.marker("AcDbBlockLinearParameter");
            g.s(305, &x.distance_name);
            g.s(306, &x.distance_description);
            g.r(140, x.distance);
            value_set(&mut g, &x.value_set, (96, 141, 175, 307));
        }
        D::LookupParameter(x) => {
            one_point(&mut g, &x.parameter);
            g.marker("AcDbBlockLookupParameter");
            g.i(94, i64::from(x.index));
            g.s(303, &x.lookup_name);
            g.s(304, &x.lookup_description);
        }
        D::PointParameter(x) => {
            one_point(&mut g, &x.parameter);
            g.marker("AcDbBlockPointParameter");
            g.s(303, &x.position_name);
            g.s(304, &x.position_description);
            g.p(1011, x.definition_label_point);
        }
        D::PolarParameter(x) => {
            two_point(&mut g, &x.parameter);
            g.marker("AcDbBlockPolarParameter");
            g.s(305, &x.distance_name);
            g.s(306, &x.distance_description);
            g.s(307, &x.angle_name);
            g.s(308, &x.angle_description);
            g.r(140, x.offset);
            value_set(&mut g, &x.distance_value_set, (96, 141, 175, 309));
            value_set(&mut g, &x.angle_value_set, (97, 145, 176, 410));
        }
        D::RotationParameter(x) => {
            two_point(&mut g, &x.parameter);
            g.marker("AcDbBlockRotationParameter");
            g.s(305, &x.angle_name);
            g.s(306, &x.angle_description);
            g.p(1011, x.definition_base_angle_point);
            g.r(140, x.angle);
            value_set(&mut g, &x.value_set, (96, 141, 175, 307));
        }
        D::XYParameter(x) => {
            two_point(&mut g, &x.parameter);
            g.marker("AcDbBlockXYParameter");
            g.s(305, &x.y_label);
            g.s(306, &x.x_label);
            g.s(307, &x.y_label_description);
            g.s(308, &x.x_label_description);
            g.r(141, x.x_value);
            g.r(140, x.y_value);
            value_set(&mut g, &x.y_value_set, (97, 146, 176, 309));
            value_set(&mut g, &x.x_value_set, (96, 142, 175, 410));
        }
        D::UserParameter(x) => {
            one_point(&mut g, &x.parameter);
            g.marker("AcDbBlockUserParameter");
            g.i(90, i64::from(x.flags));
            g.h(330, x.associated_variable);
            g.s(301, &x.expression);
            g.i(70, i64::from(x.value_code));
            match &x.value {
                BlockEvalValue::Real(v) => g.r(40, *v),
                BlockEvalValue::Text(v) => g.s(1, v),
                BlockEvalValue::Long(v) => g.i(90, i64::from(*v)),
                BlockEvalValue::Handle(v) => g.h(91, *v),
                BlockEvalValue::Short(v) => g.i(70, i64::from(*v)),
                BlockEvalValue::Point(v) => {
                    g.r(10, v[0]);
                    g.r(20, v[1]);
                }
                BlockEvalValue::None => {}
            }
            g.i(170, i64::from(x.value_type));
        }
        D::AngularConstraintParameter(x) => {
            constraint(&mut g, &x.constraint);
            g.marker("AcDbBlockAngularConstraintParameter");
            g.p(1011, x.center_point);
            g.p(1012, x.end_point);
            g.s(305, &x.expression_name);
            g.s(306, &x.expression_description);
            g.r(140, x.angle);
            g.b(280, x.orientation_on_both_grips);
            value_set(&mut g, &x.value_set, (96, 128, 175, 307));
        }
        D::DiametricConstraintParameter(x) | D::RadialConstraintParameter(x) => {
            constraint(&mut g, &x.constraint);
            g.marker(own);
            g.s(305, &x.expression_name);
            g.s(306, &x.expression_description);
            g.r(140, x.distance);
            value_set(&mut g, &x.value_set, (96, 128, 175, 307));
        }
        D::AlignedConstraintParameter(x) | D::LinearConstraintParameter(x) | D::HorizontalConstraintParameter(x) | D::VerticalConstraintParameter(x) => {
            constraint(&mut g, &x.constraint);
            g.marker("AcDbBlockLinearConstraintParameter");
            g.s(305, &x.expression_name);
            g.s(306, &x.expression_description);
            g.r(140, x.value);
            value_set(&mut g, &x.value_set, (96, 128, 175, 307));
            if !o.dxf_name.eq_ignore_ascii_case("BLOCKLINEARCONSTRAINTPARAMETER") {
                g.marker(own);
            }
        }
        D::ParameterDependencyBody(x) => {
            g.marker("AcDbAssocDependencyBody");
            g.i(90, i64::from(x.dependency_body_version));
            g.marker("AcDbImpAssocDimDependencyBodyBase");
            g.i(90, i64::from(x.dimension_base_version));
            g.s(1, &x.name);
            g.marker("AcDbBlockParameterDependencyBody");
            g.i(90, i64::from(x.class_version));
        }
        D::MoveAction(x) => {
            action(&mut g, &x.action);
            g.marker("AcDbBlockMoveAction");
            connections(&mut g, &x.connections, 92, 301);
            offsets(&mut g, &x.offsets);
        }
        D::FlipAction(x) => {
            action(&mut g, &x.action);
            g.marker("AcDbBlockFlipAction");
            connections(&mut g, &x.connections, 92, 301);
        }
        D::RotateAction(x) | D::ScaleAction(x) => {
            with_base(&mut g, &x.action);
            // The rotate action's marker is not its class name.
            g.marker(if matches!(o.data, D::RotateAction(_)) { "AcDbBlockRotationAction" } else { "AcDbBlockScaleAction" });
            connections(&mut g, &x.connections, 94, 303);
        }
        D::ArrayAction(x) => {
            action(&mut g, &x.action);
            g.marker("AcDbBlockArrayAction");
            connections(&mut g, &x.connections, 92, 301);
            g.r(140, x.row_offset);
            g.r(141, x.column_offset);
        }
        D::LookupAction(x) => {
            action(&mut g, &x.action);
            g.marker("AcDbBlockLookupAction");
            g.i(92, i64::from(x.row_count));
            g.i(93, i64::from(x.column_count));
            g.s(301, "");
            for e in &x.expressions {
                g.s(302, e);
            }
            for c in &x.columns {
                g.s(303, "");
                g.i(94, i64::from(c.node_id));
                g.i(95, i64::from(c.value_type));
                g.i(96, i64::from(c.property_type));
                g.b(282, c.lookup_property);
                g.s(305, &c.unmatched_name);
                g.b(281, c.writable);
                g.s(304, &c.connection_name);
            }
            g.b(280, x.flag_280);
        }
        D::PropertiesTable(x) => {
            use cadcraft_dwg::acadrust::objects::BlockPropertiesTableValue as V;
            one_point(&mut g, &x.parameter);
            g.marker("AcDbBlockPropertiesTable");
            for f in &x.fields {
                match &f.value {
                    V::Text(v) => g.s(f.code, v),
                    V::Boolean(v) => g.b(f.code, *v),
                    V::Integer(v) => g.i(f.code, *v),
                    V::Real(v) => g.r(f.code, *v),
                    V::Handle(v) => g.h(f.code, *v),
                    V::Binary(v) => {
                        for c in v.chunks(127) {
                            g.s(f.code, &c.iter().map(|b| format!("{b:02X}")).collect::<String>());
                        }
                    }
                }
            }
        }
        D::StretchAction(x) => {
            action(&mut g, &x.action);
            g.marker("AcDbBlockStretchAction");
            connections(&mut g, &x.connections, 92, 301);
            g.len(72, x.points.len());
            for p in &x.points {
                g.r(1011, p.x);
                g.r(1021, p.y);
            }
            g.len(73, x.handles.len());
            for h in &x.handles {
                g.h(331, h.handle);
                g.len(74, h.indexes.len());
                for i in &h.indexes {
                    g.i(94, i64::from(*i));
                }
            }
            g.len(75, x.codes.len());
            for c in &x.codes {
                g.i(95, i64::from(c.code));
                g.len(76, c.indexes.len());
                for i in &c.indexes {
                    g.i(94, i64::from(*i));
                }
            }
            offsets(&mut g, &x.offsets);
        }
        D::PolarStretchAction(x) => {
            action(&mut g, &x.action);
            g.marker("AcDbBlockPolarStretchAction");
            connections(&mut g, &x.connections, 92, 301);
            g.len(73, x.points.len());
            for p in &x.points {
                g.r(1011, p.x);
                g.r(1021, p.y);
            }
            g.len(72, x.handles.len());
            for h in &x.handles {
                g.h(331, *h);
            }
            g.len(74, x.bindings.len());
            for b in &x.bindings {
                g.h(332, b.handle);
                g.len(75, b.indexes.len());
                for i in &b.indexes {
                    g.i(76, i64::from(*i));
                }
            }
            g.len(78, x.codes.len());
            for c in &x.codes {
                g.i(98, i64::from(c.code));
                g.len(79, c.indexes.len());
                for i in &c.indexes {
                    g.i(76, i64::from(*i));
                }
            }
            g.r(141, x.distance_multiplier);
            g.r(140, x.angle_offset);
            g.i(77, 0);
        }
        D::EvaluationGraph(x) => {
            g.marker("AcDbEvalGraph");
            g.i(96, i64::from(x.first_node_id));
            g.i(97, i64::from(x.first_node_id_copy));
            for n in &x.nodes {
                g.i(91, i64::from(n.id));
                g.i(93, i64::from(n.edge_flags));
                g.i(95, i64::from(n.next_id));
                g.h(360, n.expression);
                for v in n.node_data {
                    g.i(92, i64::from(v));
                }
                if let Some(a) = n.active_cycles {
                    g.b(290, a);
                }
            }
            for e in &x.edges {
                g.i(92, i64::from(e.id));
                g.i(93, i64::from(e.next_id));
                g.i(94, i64::from(e.incoming_edge));
                g.i(91, i64::from(e.source_node));
                g.i(91, i64::from(e.destination_node));
                for v in e.outgoing_edges {
                    g.i(92, i64::from(v));
                }
            }
        }
        D::AlignmentParameterEntity
        | D::BasePointParameterEntity
        | D::FlipParameterEntity
        | D::LinearParameterEntity
        | D::PointParameterEntity
        | D::RotationParameterEntity
        | D::VisibilityParameterEntity
        | D::XYParameterEntity
        | D::FlipGripEntity
        | D::LinearGripEntity
        | D::PolarGripEntity
        | D::RotationGripEntity
        | D::VisibilityGripEntity
        | D::XYGripEntity
        | D::AngularConstraintParameterEntity(_) => {}
    }
    Some((o.dxf_name.clone(), g.0))
}
