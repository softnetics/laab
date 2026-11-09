//! Built-in LINE emoji, shared with the picker. IDs/artwork are listed at
//! https://developers.line.biz/en/docs/messaging-api/emoji-list/ and LINE's CDN.
use std::collections::HashMap;

use line_rs::client::{Decoration, Emoji, MessageInput};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Catalog {
    product_id: String,
    count: u16,
    /// The pack's own name for each emoji, e.g. `001` → "heart eyes Moon".
    alt_texts: HashMap<String, String>,
}

/// A message with one emoji. Its text is the emoji's name in parentheses,
/// e.g. "(heart eyes Moon)", which LINE shows wherever it can't draw the
/// picture: notifications, copied text, search.
pub fn input(id: &str) -> Result<MessageInput, String> {
    let catalog: Catalog = serde_json::from_str(include_str!("../../src/lib/basicEmoji.json"))
        .expect("bundled emoji catalog");
    let number = id.parse::<u16>().map_err(|_| "invalid_emoji")?;
    if number == 0 || number > catalog.count || format!("{number:03}") != id {
        return Err("invalid_emoji".into());
    }
    let name = catalog
        .alt_texts
        .get(id)
        .filter(|n| !n.is_empty())
        .map_or("unknown", String::as_str);
    Ok(MessageInput::decorated(&[Decoration::emoji(
        Emoji::new(catalog.product_id, id),
        &format!("({name})"),
    )]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_emoji_covers_the_whole_text_and_declares_ownership() {
        for id in ["001", "211"] {
            let input = input(id).unwrap();
            let text = input.text.unwrap();
            let replace: serde_json::Value =
                serde_json::from_str(&input.content_metadata["REPLACE"]).unwrap();
            let resource = &replace["sticon"]["resources"][0];
            assert_eq!(resource["S"], 0);
            assert_eq!(resource["E"], text.encode_utf16().count());
            assert_eq!(resource["sticonId"], id);
            assert_eq!(resource["resourceType"], "STATIC");
            let owners: Vec<String> =
                serde_json::from_str(&input.content_metadata["STICON_OWNERSHIP"]).unwrap();
            assert_eq!(resource["productId"], owners[0]);
            assert_eq!(owners[0], "670e0cce840a8236ddd4ee4c");
            assert!(input.e2ee.is_none()); // Keep the normal E2EE negotiation.
        }
    }

    #[test]
    fn text_is_the_emoji_name() {
        assert_eq!(input("001").unwrap().text.as_deref(), Some("(heart eyes Moon)"));
        assert_eq!(input("211").unwrap().text.as_deref(), Some("(nine)"));
    }

    #[test]
    fn every_emoji_has_a_name() {
        let catalog: Catalog =
            serde_json::from_str(include_str!("../../src/lib/basicEmoji.json")).unwrap();
        for n in 1..=catalog.count {
            assert!(catalog.alt_texts.contains_key(&format!("{n:03}")), "{n:03}");
        }
    }

    #[test]
    fn rejects_unknown_or_noncanonical_emoji() {
        for id in ["", "000", "212", "1", "0001", "../001", "abc"] {
            assert!(input(id).is_err(), "{id}");
        }
    }
}
