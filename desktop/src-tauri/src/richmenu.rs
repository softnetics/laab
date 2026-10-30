//! The rich menu of an official account: the image under the composer with
//! tap areas. `getBuddyChatBarV2` returns it as a JSON `body` with a canvas,
//! scenes of drawn images and touch listeners, and named actions.

use serde::Serialize;
use serde_json::Value as Json;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RichMenu {
    /// The chat bar label, `เมนู` in the LINE app.
    pub label: String,
    pub width: f64,
    pub height: f64,
    pub images: Vec<RichMenuImage>,
    pub areas: Vec<RichMenuArea>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RichMenuImage {
    pub url: String,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RichMenuArea {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub action: RichMenuAction,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RichMenuAction {
    /// LINE's action type: `web` for links. Others are passed through.
    #[serde(rename = "type")]
    pub kind: String,
    /// The area's name, used as its accessible label.
    pub label: Option<String>,
    pub uri: Option<String>,
}

/// The first rich menu item of a `getBuddyChatBarV2` result, or `None` if
/// the account has none or its body can't be read.
pub fn from_chat_bar(bar: &Json) -> Option<RichMenu> {
    let rich = bar["barItems"]
        .as_array()?
        .iter()
        .find_map(|item| item.get("rich"))?;
    let body: Json = serde_json::from_str(rich["body"].as_str()?).ok()?;
    parse(&body, rich["label"].as_str().unwrap_or("เมนู"))
}

fn parse(body: &Json, label: &str) -> Option<RichMenu> {
    let canvas = &body["canvas"];
    let scene_name = canvas["initialScene"].as_str()?;
    let scene = &body["scenes"][scene_name];
    let images = scene["draws"]
        .as_array()?
        .iter()
        .filter_map(|d| {
            let image = &body["images"][d["image"].as_str()?];
            Some(RichMenuImage {
                url: image["imageUrl"].as_str()?.to_string(),
                x: d["x"].as_f64()?,
                y: d["y"].as_f64()?,
                w: d["w"].as_f64()?,
                h: d["h"].as_f64()?,
            })
        })
        .collect::<Vec<_>>();
    if images.is_empty() {
        return None;
    }
    let areas = scene["listeners"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|l| l["type"] == "touch")
        .filter_map(|l| {
            let [x, y, w, h] = l["params"].as_array()?.as_slice() else {
                return None;
            };
            let action = &body["actions"][l["action"].as_str()?];
            Some(RichMenuArea {
                x: x.as_f64()?,
                y: y.as_f64()?,
                w: w.as_f64()?,
                h: h.as_f64()?,
                action: RichMenuAction {
                    kind: action["type"].as_str().unwrap_or("").to_string(),
                    label: action["text"].as_str().map(String::from),
                    uri: action["params"]["linkUri"].as_str().map(String::from),
                },
            })
        })
        .collect();
    Some(RichMenu {
        label: label.to_string(),
        width: canvas["width"].as_f64()?,
        height: canvas["height"].as_f64()?,
        images,
        areas,
    })
}

/// A two-button menu for the demo bot, drawn as an SVG.
pub fn demo() -> RichMenu {
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="2500" height="843"><rect width="2500" height="843" fill="#1F7A4D"/><rect x="1249" width="2" height="843" fill="#fff" opacity=".4"/><g fill="#fff" font-family="sans-serif" font-size="150" text-anchor="middle"><text x="625" y="470">ติดตามพัสดุ</text><text x="1875" y="470">สาขาใกล้คุณ</text></g></svg>"##;
    let url = format!(
        "data:image/svg+xml;base64,{}",
        base64::Engine::encode(&base64::engine::general_purpose::STANDARD, svg)
    );
    let area = |x: f64, label: &str, uri: &str| RichMenuArea {
        x,
        y: 0.0,
        w: 1250.0,
        h: 843.0,
        action: RichMenuAction {
            kind: "web".into(),
            label: Some(label.into()),
            uri: Some(uri.into()),
        },
    };
    RichMenu {
        label: "เมนู".into(),
        width: 2500.0,
        height: 843.0,
        images: vec![RichMenuImage {
            url,
            x: 0.0,
            y: 0.0,
            w: 2500.0,
            h: 843.0,
        }],
        areas: vec![
            area(0.0, "ติดตามพัสดุ", "https://liff.line.me/1234567890-demoTrak"),
            area(1250.0, "สาขาใกล้คุณ", "https://example.com/branches"),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn bar(body: Json) -> Json {
        json!({ "barItems": [{ "rich": { "label": "เมนู", "selected": true, "body": body.to_string() } }] })
    }

    #[test]
    fn reads_image_and_tap_areas() {
        let body = json!({
            "canvas": { "initialScene": "scene1", "width": 2500, "height": 1686 },
            "scenes": { "scene1": {
                "draws": [{ "image": "image0", "x": 0, "y": 0, "w": 2500, "h": 1686 }],
                "listeners": [
                    { "type": "touch", "params": [0, 0, 833, 843], "action": "action0" },
                    { "type": "touch", "params": [833, 0, 833, 843], "action": "action1" },
                    { "type": "touch", "params": [0, 0], "action": "action0" },
                ],
            }},
            "images": { "image0": { "x": 0, "y": 0, "w": 2500, "h": 1686, "imageUrl": "https://cdn.example/menu" } },
            "actions": {
                "action0": { "action": "action0", "type": "web", "text": "ร้องเรียน", "params": { "linkUri": "https://liff.line.me/123-abc/report" } },
                "action1": { "action": "action1", "type": "text", "text": "สวัสดี", "params": {} },
            },
        });
        let menu = from_chat_bar(&bar(body)).unwrap();
        assert_eq!(menu.label, "เมนู");
        assert_eq!((menu.width, menu.height), (2500.0, 1686.0));
        assert_eq!(menu.images[0].url, "https://cdn.example/menu");
        assert_eq!(menu.areas.len(), 2);
        assert_eq!(
            menu.areas[0].action.uri.as_deref(),
            Some("https://liff.line.me/123-abc/report")
        );
        assert_eq!(menu.areas[0].action.label.as_deref(), Some("ร้องเรียน"));
        assert_eq!((menu.areas[1].x, menu.areas[1].w), (833.0, 833.0));
        assert_eq!(menu.areas[1].action.kind, "text");
        assert_eq!(menu.areas[1].action.uri, None);
    }

    #[test]
    fn no_menu_without_a_rich_item_or_image() {
        assert_eq!(from_chat_bar(&json!({ "barItems": [] })), None);
        assert_eq!(from_chat_bar(&json!({ "barItems": [{ "web": {} }] })), None);
        assert_eq!(
            from_chat_bar(&bar(
                json!({ "canvas": { "initialScene": "s" }, "scenes": { "s": { "draws": [] } } })
            )),
            None
        );
    }
}
