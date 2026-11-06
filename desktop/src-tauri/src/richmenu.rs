//! The rich menu of an official account: the image under the composer with
//! tap areas. `getBuddyChatBarV2` returns it as a JSON `body` with a canvas,
//! scenes of drawn images and touch listeners, and named actions.

use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::Value as Json;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RichMenu {
    /// The chat bar label, `เมนู` in the LINE app.
    pub label: String,
    pub width: f64,
    pub height: f64,
    /// The scene shown first. Areas with a `transition` action switch to
    /// another one.
    pub initial_scene: String,
    pub scenes: BTreeMap<String, RichMenuScene>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RichMenuScene {
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

/// What tapping an area does. LINE's action types (`uo8/a.java`): `web`
/// opens `uri`, `postback` sends `data` (a `linepostback://` URL) to the
/// bot, `sendMessage` sends `text` as the user, `transition` shows
/// `scene`, `clipboard` copies `clipboard_text`, and `app` is for the
/// phone.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RichMenuAction {
    #[serde(rename = "type")]
    pub kind: String,
    /// The area's name, used as its accessible label.
    pub label: Option<String>,
    pub uri: Option<String>,
    pub data: Option<String>,
    pub text: Option<String>,
    pub scene: Option<String>,
    pub clipboard_text: Option<String>,
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

fn text_of(v: &Json) -> Option<String> {
    v.as_str().filter(|s| !s.is_empty()).map(String::from)
}

/// An entry of the body's `actions`: `{type, text (its name), params}`.
fn action(a: &Json) -> RichMenuAction {
    let params = &a["params"];
    let kind = a["type"].as_str().unwrap_or("").to_string();
    let mut out = RichMenuAction {
        label: text_of(&a["text"]),
        ..Default::default()
    };
    match kind.as_str() {
        "web" => out.uri = text_of(&params["linkUri"]).or_else(|| text_of(&params["httpUri"])),
        "postback" => out.data = text_of(&params["linkUri"]),
        "sendMessage" => out.text = text_of(&params["text"]),
        "transition" => out.scene = text_of(&params["nextScene"]),
        "clipboard" => out.clipboard_text = text_of(&params["clipboardText"]),
        _ => out.uri = text_of(&params["linkUri"]),
    }
    out.kind = kind;
    out
}

fn scene(body: &Json, scene: &Json) -> Option<RichMenuScene> {
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
            Some(RichMenuArea {
                x: x.as_f64()?,
                y: y.as_f64()?,
                w: w.as_f64()?,
                h: h.as_f64()?,
                action: action(&body["actions"][l["action"].as_str()?]),
            })
        })
        .collect();
    Some(RichMenuScene { images, areas })
}

fn parse(body: &Json, label: &str) -> Option<RichMenu> {
    let canvas = &body["canvas"];
    let initial = canvas["initialScene"].as_str()?;
    let scenes: BTreeMap<String, RichMenuScene> = body["scenes"]
        .as_object()?
        .iter()
        .filter_map(|(name, s)| Some((name.clone(), scene(body, s)?)))
        .collect();
    scenes.get(initial)?;
    Some(RichMenu {
        label: label.to_string(),
        width: canvas["width"].as_f64()?,
        height: canvas["height"].as_f64()?,
        initial_scene: initial.to_string(),
        scenes,
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
            ..Default::default()
        },
    };
    let main = RichMenuScene {
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
    };
    RichMenu {
        label: "เมนู".into(),
        width: 2500.0,
        height: 843.0,
        initial_scene: "main".into(),
        scenes: BTreeMap::from([("main".to_string(), main)]),
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
    fn reads_images_scenes_and_every_action() {
        let body = json!({
            "canvas": { "initialScene": "scene1", "width": 2500, "height": 1686 },
            "scenes": {
                "scene1": {
                    "draws": [{ "image": "image0", "x": 0, "y": 0, "w": 2500, "h": 1686 }],
                    "listeners": [
                        { "type": "touch", "params": [0, 0, 833, 843], "action": "web" },
                        { "type": "touch", "params": [833, 0, 833, 843], "action": "say" },
                        { "type": "touch", "params": [1666, 0, 834, 843], "action": "post" },
                        { "type": "touch", "params": [0, 843, 1250, 843], "action": "next" },
                        { "type": "touch", "params": [1250, 843, 1250, 843], "action": "copy" },
                        { "type": "touch", "params": [0, 0], "action": "web" },
                    ],
                },
                "scene2": {
                    "draws": [{ "image": "image1", "x": 0, "y": 0, "w": 2500, "h": 1686 }],
                    "listeners": [{ "type": "touch", "params": [0, 0, 2500, 1686], "action": "back" }],
                },
            },
            "images": {
                "image0": { "imageUrl": "https://cdn.example/menu" },
                "image1": { "imageUrl": "https://cdn.example/menu2" },
            },
            "actions": {
                "web": { "type": "web", "text": "ร้องเรียน", "params": { "linkUri": "https://liff.line.me/123-abc/report" } },
                "say": { "type": "sendMessage", "text": "ทักทาย", "params": { "text": "สวัสดี" } },
                "post": { "type": "postback", "text": "คูปอง", "params": { "linkUri": "linepostback://postback?_data=coupon" } },
                "next": { "type": "transition", "text": "หน้าถัดไป", "params": { "nextScene": "scene2" } },
                "copy": { "type": "clipboard", "text": "โค้ด", "params": { "clipboardText": "ABC123" } },
                "back": { "type": "transition", "params": { "nextScene": "scene1" } },
            },
        });
        let menu = from_chat_bar(&bar(body)).unwrap();
        assert_eq!(menu.label, "เมนู");
        assert_eq!((menu.width, menu.height), (2500.0, 1686.0));
        assert_eq!(menu.initial_scene, "scene1");
        let first = &menu.scenes["scene1"];
        assert_eq!(first.images[0].url, "https://cdn.example/menu");
        assert_eq!(first.areas.len(), 5);
        let a: Vec<&RichMenuAction> = first.areas.iter().map(|a| &a.action).collect();
        assert_eq!(a[0].uri.as_deref(), Some("https://liff.line.me/123-abc/report"));
        assert_eq!(a[0].label.as_deref(), Some("ร้องเรียน"));
        assert_eq!((a[1].kind.as_str(), a[1].text.as_deref()), ("sendMessage", Some("สวัสดี")));
        assert_eq!(a[2].data.as_deref(), Some("linepostback://postback?_data=coupon"));
        assert_eq!(a[2].uri, None);
        assert_eq!(a[3].scene.as_deref(), Some("scene2"));
        assert_eq!(a[4].clipboard_text.as_deref(), Some("ABC123"));
        assert_eq!((first.areas[1].x, first.areas[1].w), (833.0, 833.0));
        assert_eq!(menu.scenes["scene2"].areas[0].action.scene.as_deref(), Some("scene1"));
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
