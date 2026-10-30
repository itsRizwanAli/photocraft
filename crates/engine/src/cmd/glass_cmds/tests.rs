use photocraft_doc::{LayerContent, LayerId};
use serde_json::json;

use crate::Session;

fn backdrop() -> Session {
    let mut s = Session::new();
    s.execute("file.new", json!({"width": 200, "height": 160, "background": "#3060c0"})).unwrap();
    s.execute("shape.create", json!({"kind": "ellipse", "rect": [40, 30, 80, 80], "fill": "#ff8020"})).unwrap();
    s
}

#[test]
fn glass_card_is_a_blurred_masked_smart_object_under_a_styled_shape_in_one_step() {
    let mut s = backdrop();
    let steps = s.active().unwrap().history.past_len();
    let layers = s.active().unwrap().doc.layers.len();
    let r = s.execute("shape.glassCard", json!({"rect": [20, 20, 160, 100], "radius": 20, "blur": 12})).unwrap();
    let d = s.active().unwrap();
    assert_eq!(d.history.past_len(), steps + 1, "one undo step");
    assert_eq!(d.doc.layers.len(), layers + 2);
    let blur = d.doc.layer(LayerId(r["blur"].as_u64().unwrap())).unwrap();
    let LayerContent::Smart(sm) = &blur.content else { panic!("the backdrop is a smart object") };
    assert_eq!(sm.smart_filters.len(), 1, "with the blur as a smart filter");
    assert!(blur.vector_mask.is_some(), "masked to the card");
    assert_eq!(blur.name, "Glass Card Blur");
    let card = d.doc.layer(LayerId(r["card"].as_u64().unwrap())).unwrap();
    assert!(matches!(card.content, LayerContent::Shape(_)), "the card is a live shape");
    assert_eq!(card.effects.items.len(), 2, "inner glow and drop shadow");
    assert!((card.fill_opacity - 0.18).abs() < 1e-3, "the tint shows through at Fill opacity: {}", card.fill_opacity);
    // The card sits above its blur.
    let pos = |id: LayerId| d.doc.layers.iter().position(|l| l.id == id).unwrap();
    assert!(pos(card.id) > pos(blur.id));
    // One Undo takes the whole card back.
    assert!(s.undo());
    assert_eq!(s.active().unwrap().doc.layers.len(), layers);
}

#[test]
fn glass_card_options_and_bad_params() {
    let mut s = backdrop();
    let r = s.execute("shape.glassCard", json!({"rect": [10, 10, 50, 40], "glowOpacity": 0, "shadowOpacity": 0, "strokeWidth": 0, "name": "Pill"})).unwrap();
    let d = s.active().unwrap();
    let card = d.doc.layer(LayerId(r["card"].as_u64().unwrap())).unwrap();
    assert!(card.effects.items.is_empty(), "no effects asked for");
    assert_eq!(card.name, "Pill");
    for bad in [json!({}), json!({"rect": [0, 0, 0, 10]}), json!({"rect": [0, 0, 10, 10], "tint": "white"}), json!({"rect": [0, 0, 10, 10], "blur": -1})] {
        let layers = s.active().unwrap().doc.layers.len();
        assert!(s.execute("shape.glassCard", bad.clone()).is_err(), "{bad}");
        assert_eq!(s.active().unwrap().doc.layers.len(), layers, "{bad}: nothing is left behind");
    }
}
