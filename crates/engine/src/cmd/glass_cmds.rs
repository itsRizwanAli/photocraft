//! Glass card (glassmorphism) in one command and one undo step, built only from layers Photoshop
//! has, so it opens and stays editable there:
//!
//! - "Glass Blur": Stamp Visible as a smart object with a Gaussian Blur smart filter, masked to
//!   the card by a vector mask (the frosted backdrop; edit the blur radius as a smart filter).
//! - "Glass Card": a rounded-rectangle shape layer above it with a translucent tint, a thin light
//!   stroke, and Inner Glow and Drop Shadow effects.
//!
//! The backdrop is a snapshot of what was visible: after changing the layers under the card,
//! run the command again (Photoshop has no live backdrop blur either).

use serde_json::{Value, json};

use crate::commands::CommandSpec;
use crate::extra_cmds::step_key;
use crate::{EngineError, Result, Session};

const CMD: &str = "shape.glassCard";

fn has_doc(s: &Session) -> std::result::Result<(), String> {
    s.active().map(|_| ()).ok_or_else(|| "no document open".into())
}

fn bad(m: impl Into<String>) -> EngineError {
    EngineError::BadParams { cmd: CMD.into(), msg: m.into() }
}

fn num(p: &Value, key: &str, default: f64, range: std::ops::RangeInclusive<f64>) -> Result<f64> {
    match p.get(key) {
        None | Some(Value::Null) => Ok(default),
        Some(v) => v.as_f64().filter(|n| n.is_finite() && range.contains(n)).ok_or_else(|| bad(format!("\"{key}\" must be a number in {range:?}"))),
    }
}

/// `"#rrggbb"` → `[r, g, b]` in 0..1.
fn hex_rgb(p: &Value, key: &str, default: &str) -> Result<[f64; 3]> {
    let s = p.get(key).and_then(Value::as_str).unwrap_or(default);
    let h = s.strip_prefix('#').unwrap_or(s);
    let byte = |i: usize| h.get(i..i + 2).and_then(|b| u8::from_str_radix(b, 16).ok());
    match (h.len(), byte(0), byte(2), byte(4)) {
        (6, Some(r), Some(g), Some(b)) => Ok([r, g, b].map(|c| f64::from(c) / 255.0)),
        _ => Err(bad(format!("\"{key}\" must be a colour like \"#ffffff\""))),
    }
}

fn layer_of(v: &Value) -> Result<u64> {
    v.get("layer").and_then(Value::as_u64).ok_or_else(|| EngineError::Other("a step of the glass card made no layer".into()))
}

fn glass_card(s: &mut Session, p: &Value) -> Result<Value> {
    let rect = p
        .get("rect")
        .and_then(Value::as_array)
        .filter(|r| r.len() == 4 && r.iter().all(|v| v.as_f64().is_some_and(f64::is_finite)))
        .ok_or_else(|| bad("\"rect\" must be [x, y, width, height]"))?
        .clone();
    if rect[2].as_f64().unwrap_or(0.0) <= 0.0 || rect[3].as_f64().unwrap_or(0.0) <= 0.0 {
        return Err(bad("the card's width and height must be positive"));
    }
    let radius = num(p, "radius", 24.0, 0.0..=10_000.0)?;
    let blur = num(p, "blur", 24.0, 0.1..=1000.0)?;
    let tint = hex_rgb(p, "tint", "#ffffff")?;
    let tint_opacity = num(p, "tintOpacity", 18.0, 0.0..=100.0)?;
    let stroke_width = num(p, "strokeWidth", 1.5, 0.0..=100.0)?;
    let stroke_opacity = num(p, "strokeOpacity", 45.0, 0.0..=100.0)?;
    let glow_opacity = num(p, "glowOpacity", 35.0, 0.0..=100.0)?;
    let shadow_opacity = num(p, "shadowOpacity", 25.0, 0.0..=100.0)?;
    let shadow_size = num(p, "shadowSize", 40.0, 0.0..=250.0)?;
    let shadow_distance = num(p, "shadowDistance", 16.0, 0.0..=30_000.0)?;
    let name = p.get("name").and_then(Value::as_str).unwrap_or("Glass Card").to_string();
    let stroke_color = p.get("strokeColor").and_then(Value::as_str).unwrap_or("#ffffff").to_string();
    hex_rgb(p, "strokeColor", "#ffffff")?;

    let key = step_key(s, "glassCard");
    let k = json!(key);
    // 1. The frosted backdrop: what shows now, as a smart object blurred by a smart filter.
    // The stamp is the active layer; converting it makes a new layer (a new id) holding it.
    layer_of(&s.execute("layer.stampVisible", json!({"coalesce": k}))?)?;
    let blur_layer = layer_of(&s.execute("layer.smartObjects.convertToSmartObject", json!({"coalesce": k}))?)?;
    s.execute("filter.blur.gaussianBlur", json!({"radius": blur, "coalesce": k}))?;
    s.execute("layer.setProps", json!({"layer": blur_layer, "name": format!("{name} Blur"), "coalesce": k}))?;
    // 2. The card above it: translucent tint and a thin light edge.
    let mut stroke = json!({"width": stroke_width, "color": stroke_color, "opacity": stroke_opacity, "align": "inside"});
    if stroke_width == 0.0 {
        stroke = Value::Null;
    }
    let fill = json!([tint[0], tint[1], tint[2], tint_opacity / 100.0]);
    let card = s.execute("shape.create", json!({"kind": "roundedRect", "rect": rect, "radii": radius, "fill": fill, "stroke": stroke, "name": name, "coalesce": k}))?;
    let card_layer = layer_of(&card)?;
    // 3. Mask the blur to the card's own outline, so moving or reshaping stays one path edit.
    let path = card.get("path").cloned().ok_or_else(|| EngineError::Other("the card shape has no path".into()))?;
    s.execute("layer.vectorMask.add", json!({"layer": blur_layer, "path": path, "coalesce": k}))?;
    // 4. Light from the edges and a soft shadow; the shape knocks the shadow out under itself.
    let mut effects = Vec::new();
    if glow_opacity > 0.0 {
        effects.push(json!({"kind": "innerGlow", "params": {"color": "#ffffff", "opacity": glow_opacity, "blend": "screen", "source": "edge", "size": (radius * 0.5).clamp(4.0, 60.0)}}));
    }
    if shadow_opacity > 0.0 {
        effects.push(json!({"kind": "dropShadow", "params": {"color": "#000000", "opacity": shadow_opacity, "blend": "multiply", "angle": 90, "useGlobalLight": false, "distance": shadow_distance, "size": shadow_size, "knocksOut": true}}));
    }
    if !effects.is_empty() {
        s.execute("layer.layerStyle.replace", json!({"layer": card_layer, "effects": effects, "coalesce": k}))?;
    }
    Ok(json!({"card": card_layer, "blur": blur_layer}))
}

pub fn specs() -> Vec<CommandSpec> {
    vec![CommandSpec {
        id: CMD,
        label: "Glass Card",
        menu: &[],
        shortcut: None,
        params: r##"{"rect":[x,y,w,h],"radius":px=24,"blur":px=24 (Gaussian Blur smart filter on the backdrop),"tint":"#rrggbb"="#ffffff","tintOpacity":0..100=18,"strokeWidth":px=1.5 (0: none),"strokeColor":"#rrggbb"="#ffffff","strokeOpacity":0..100=45,"glowOpacity":0..100=35 (Inner Glow, 0: none),"shadowOpacity":0..100=25 (Drop Shadow, 0: none),"shadowSize":px=40,"shadowDistance":px=16,"name":str="Glass Card"} → {card, blur}: a frosted-glass card (glassmorphism) as a blurred smart-object backdrop masked to a rounded-rectangle shape layer with tint, stroke, inner glow and drop shadow; one undo step, all Photoshop-native layers"##,
        enabled: has_doc,
        journal: true,
        run: glass_card,
    }]
}

#[cfg(test)]
#[path = "glass_cmds/tests.rs"]
mod tests;
