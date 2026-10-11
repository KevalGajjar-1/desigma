use crate::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Node {
    pub id: NodeId,
    /// Original id from an imported file (Figma "12:34"), for round-tripping.
    pub source_id: Option<String>,
    pub name: String,
    pub kind: NodeKind,
    pub parent: Option<NodeId>,
    /// Back-to-front order.
    pub children: Vec<NodeId>,
    pub visible: bool,
    pub locked: bool,
    pub opacity: f32,
    pub blend: BlendMode,
    /// Transform relative to the parent (Figma `relativeTransform`).
    pub transform: Affine,
    /// Untransformed size.
    pub size: Vec2,
    pub fills: Vec<Paint>,
    pub stroke: Stroke,
    pub effects: Vec<Effect>,
    /// Corner radii (top-left, top-right, bottom-right, bottom-left).
    pub corner_radii: [f32; 4],
    /// iOS-style corner smoothing 0..1.
    pub corner_smoothing: f32,
    pub constraints: Constraints,
    /// Child behaviour inside an auto-layout parent.
    pub layout_child: LayoutChild,
    pub is_mask: bool,
    /// Style id per property ("fill", "stroke", "text", "effect", "grid").
    pub style_refs: BTreeMap<String, String>,
    /// Precomputed fill outline in local space (Figma `fillGeometry`). When
    /// present, renderers may use it instead of deriving geometry, e.g. for
    /// boolean operations whose result was computed by Figma.
    #[serde(default)]
    pub fill_geometry: Option<Vec<Path>>,
    /// Precomputed stroke outline in local space (Figma `strokeGeometry`),
    /// already expanded by weight/align; filled with the stroke paints.
    #[serde(default)]
    pub stroke_geometry: Option<Vec<Path>>,
    /// Component/instance bookkeeping; empty for ordinary nodes.
    #[serde(default, skip_serializing_if = "ComponentLink::is_empty")]
    pub component: ComponentLink,
}

/// A node property that an instance can override independently of its main
/// component. Syncing a component into its instances copies every field that
/// is not in the instance node's [`ComponentLink::overrides`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum OverrideField {
    Name,
    Visible,
    Locked,
    Opacity,
    Blend,
    Size,
    Transform,
    Fills,
    Stroke,
    Effects,
    CornerRadii,
    /// Text characters.
    Characters,
    /// Text style and runs.
    TextStyle,
    /// Frame props: clipping, auto layout, grids.
    Layout,
    /// Which component an instance points at (instance swap).
    MainComponent,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum PropValue {
    Bool(bool),
    Text(String),
    /// Selected option of a variant property.
    Variant(String),
    /// Component swapped in, by node id.
    Instance(NodeId),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum PropKind {
    Boolean,
    Text,
    Variant { options: Vec<String> },
    InstanceSwap { preferred: Vec<NodeId> },
}

/// Component property definition, on a Component or ComponentSet.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ComponentPropDef {
    pub name: String,
    pub kind: PropKind,
    pub default: PropValue,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ComponentLink {
    /// For a node inside an instance: the node of the main component it mirrors.
    /// Instance roots point at their component via `NodeKind::Instance::component`.
    pub source: Option<NodeId>,
    /// Properties changed on this instance node; preserved when syncing.
    pub overrides: BTreeSet<OverrideField>,
    /// Property definitions (Component / ComponentSet only).
    pub props: Vec<ComponentPropDef>,
    /// Property values chosen on an instance root, by property name.
    pub values: BTreeMap<String, PropValue>,
    /// Binds a node property to a component property, e.g.
    /// `Visible -> "Show icon"`, `Characters -> "Label"`, `MainComponent -> "Icon"`.
    pub bindings: BTreeMap<OverrideField, String>,
}

impl ComponentLink {
    pub fn is_empty(&self) -> bool {
        self.source.is_none()
            && self.overrides.is_empty()
            && self.props.is_empty()
            && self.values.is_empty()
            && self.bindings.is_empty()
    }
}

impl Node {
    pub fn new(id: NodeId, kind: NodeKind) -> Self {
        Self {
            id,
            source_id: None,
            name: kind.default_name().into(),
            kind,
            parent: None,
            children: Vec::new(),
            visible: true,
            locked: false,
            opacity: 1.0,
            blend: BlendMode::PassThrough,
            transform: Affine::IDENTITY,
            size: Vec2::ZERO,
            fills: Vec::new(),
            stroke: Stroke { weight: 1.0, miter_limit: 4.0, ..Default::default() },
            effects: Vec::new(),
            corner_radii: [0.0; 4],
            corner_smoothing: 0.0,
            constraints: Constraints::default(),
            layout_child: LayoutChild::default(),
            is_mask: false,
            style_refs: BTreeMap::new(),
            fill_geometry: None,
            stroke_geometry: None,
            component: ComponentLink::default(),
        }
    }

    fn placed(id: NodeId, kind: NodeKind, r: Rect) -> Self {
        let mut n = Self::new(id, kind);
        n.transform = Affine::translate(r.x, r.y);
        n.size = Vec2::new(r.w, r.h);
        n
    }

    pub fn frame(id: NodeId, r: Rect) -> Self {
        let mut n = Self::placed(id, NodeKind::Frame(FrameProps::default()), r);
        n.fills = vec![Paint::solid(Color::WHITE)];
        n
    }
    pub fn rectangle(id: NodeId, r: Rect) -> Self {
        let mut n = Self::placed(id, NodeKind::Rectangle, r);
        n.fills = vec![Paint::solid(Color::rgb(0xD9, 0xD9, 0xD9))];
        n
    }
    pub fn ellipse(id: NodeId, r: Rect) -> Self {
        let mut n = Self::placed(id, NodeKind::Ellipse(ArcData::default()), r);
        n.fills = vec![Paint::solid(Color::rgb(0xD9, 0xD9, 0xD9))];
        n
    }
    pub fn text(id: NodeId, r: Rect, content: impl Into<String>, style: TextStyle) -> Self {
        Self::placed(
            id,
            NodeKind::Text(TextProps {
                content: content.into(),
                style,
                runs: Vec::new(),
                auto_resize: TextAutoResize::WidthAndHeight,
                max_lines: None,
            }),
            r,
        )
    }

    /// Axis-aligned local bounds (0, 0, w, h).
    pub fn local_bounds(&self) -> Rect {
        Rect::new(0.0, 0.0, self.size.x, self.size.y)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum NodeKind {
    Page { background: Color },
    Frame(FrameProps),
    Group,
    Section,
    Rectangle,
    Ellipse(ArcData),
    Line,
    /// Regular polygon with `count` sides.
    Polygon { count: u32 },
    Star { count: u32, inner_ratio: f32 },
    /// Arbitrary vector geometry in local space.
    Vector { paths: Vec<Path> },
    Text(TextProps),
    BooleanOp(BooleanOp),
    /// Main component; frame-like.
    Component(FrameProps),
    ComponentSet(FrameProps),
    /// Instance of a component; children are the resolved (overridden) copy.
    Instance { frame: FrameProps, component: Option<NodeId>, component_key: Option<String> },
    /// Anything the importer could not map, kept so nothing is silently lost.
    Unknown { type_name: String },
}

impl NodeKind {
    pub fn is_container(&self) -> bool {
        matches!(
            self,
            NodeKind::Page { .. }
                | NodeKind::Frame(_)
                | NodeKind::Group
                | NodeKind::Section
                | NodeKind::BooleanOp(_)
                | NodeKind::Component(_)
                | NodeKind::ComponentSet(_)
                | NodeKind::Instance { .. }
        )
    }

    pub fn frame_props(&self) -> Option<&FrameProps> {
        match self {
            NodeKind::Frame(f) | NodeKind::Component(f) | NodeKind::ComponentSet(f) => Some(f),
            NodeKind::Instance { frame, .. } => Some(frame),
            _ => None,
        }
    }

    pub fn frame_props_mut(&mut self) -> Option<&mut FrameProps> {
        match self {
            NodeKind::Frame(f) | NodeKind::Component(f) | NodeKind::ComponentSet(f) => Some(f),
            NodeKind::Instance { frame, .. } => Some(frame),
            _ => None,
        }
    }

    pub fn default_name(&self) -> &'static str {
        match self {
            NodeKind::Page { .. } => "Page",
            NodeKind::Frame(_) => "Frame",
            NodeKind::Group => "Group",
            NodeKind::Section => "Section",
            NodeKind::Rectangle => "Rectangle",
            NodeKind::Ellipse(_) => "Ellipse",
            NodeKind::Line => "Line",
            NodeKind::Polygon { .. } => "Polygon",
            NodeKind::Star { .. } => "Star",
            NodeKind::Vector { .. } => "Vector",
            NodeKind::Text(_) => "Text",
            NodeKind::BooleanOp(_) => "Union",
            NodeKind::Component(_) => "Component",
            NodeKind::ComponentSet(_) => "Component set",
            NodeKind::Instance { .. } => "Instance",
            NodeKind::Unknown { .. } => "Layer",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BooleanOp {
    Union,
    Subtract,
    Intersect,
    Exclude,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ArcData {
    pub start: f32,
    pub end: f32,
    pub inner_radius: f32,
}

impl Default for ArcData {
    fn default() -> Self {
        Self { start: 0.0, end: std::f32::consts::TAU, inner_radius: 0.0 }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TextProps {
    pub content: String,
    pub style: TextStyle,
    /// Style overrides over byte ranges of `content`; empty means uniform.
    pub runs: Vec<TextRun>,
    pub auto_resize: TextAutoResize,
    /// Maximum visible lines when `auto_resize` is `Truncate`; overflow ends in "…".
    /// `None` truncates at the box height.
    #[serde(default)]
    pub max_lines: Option<u32>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct FrameProps {
    pub clips_content: bool,
    pub auto_layout: Option<AutoLayout>,
    pub grids: Vec<LayoutGrid>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum LayoutDirection {
    #[default]
    Horizontal,
    Vertical,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum LayoutAlign {
    #[default]
    Start,
    Center,
    End,
    SpaceBetween,
    Baseline,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Sizing {
    #[default]
    Fixed,
    Hug,
    Fill,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AutoLayout {
    pub direction: LayoutDirection,
    pub wrap: bool,
    pub item_spacing: f32,
    pub counter_spacing: f32,
    /// top, right, bottom, left
    pub padding: [f32; 4],
    pub primary_align: LayoutAlign,
    pub counter_align: LayoutAlign,
    pub primary_sizing: Sizing,
    pub counter_sizing: Sizing,
    pub stroke_included: bool,
    pub reverse_z: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LayoutChild {
    pub absolute: bool,
    pub horizontal: Sizing,
    pub vertical: Sizing,
    pub grow: f32,
    pub min_w: Option<f32>,
    pub max_w: Option<f32>,
    pub min_h: Option<f32>,
    pub max_h: Option<f32>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Constraint {
    #[default]
    Min,
    Max,
    Center,
    Stretch,
    Scale,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Constraints {
    pub horizontal: Constraint,
    pub vertical: Constraint,
}
