//! Read cursors. A receipt means "read through this message", so a cursor
//! only ever moves forward. LINE message ids are 64-bit digit strings and are
//! compared exactly, never as floats.

use std::cmp::Ordering;

pub fn is_numeric_id(v: &str) -> bool {
    !v.is_empty() && v.bytes().all(|b| b.is_ascii_digit())
}

pub fn compare_ids(a: &str, b: &str) -> Option<Ordering> {
    if !is_numeric_id(a) || !is_numeric_id(b) {
        return None;
    }
    let strip = |s: &str| {
        let t = s.trim_start_matches('0');
        if t.is_empty() {
            "0".to_string()
        } else {
            t.to_string()
        }
    };
    let (x, y) = (strip(a), strip(b));
    Some(x.len().cmp(&y.len()).then_with(|| x.cmp(&y)))
}

/// A newer receipt wins. An unusable id never pulls a cursor backwards.
pub fn later(current: Option<&str>, incoming: &str) -> String {
    if incoming.is_empty() {
        return current.unwrap_or("").to_string();
    }
    let Some(current) = current.filter(|c| !c.is_empty()) else {
        return incoming.to_string();
    };
    match compare_ids(incoming, current) {
        Some(Ordering::Greater) => incoming.to_string(),
        Some(_) => current.to_string(),
        None if is_numeric_id(incoming) && !is_numeric_id(current) => incoming.to_string(),
        None => current.to_string(),
    }
}

/// Numeric receipts place themselves; anything else means "the latest
/// message we hold".
pub fn cursor(reported: Option<&str>, fallback: Option<&str>) -> Option<String> {
    match reported {
        Some(r) if is_numeric_id(r) => Some(r.to_string()),
        _ => fallback.filter(|f| !f.is_empty()).map(str::to_string),
    }
}

/// What is still unread after we read a chat through `cursor`, on this or
/// another device: the messages from others newer than it, never more than
/// `current`. A cursor we can't place means the whole chat was read.
pub fn unread_after<'a>(
    current: i64,
    cursor: &str,
    from_others: impl IntoIterator<Item = &'a str>,
) -> i64 {
    if !is_numeric_id(cursor) {
        return 0;
    }
    let newer = from_others
        .into_iter()
        .filter(|id| compare_ids(id, cursor) == Some(Ordering::Greater))
        .count() as i64;
    newer.min(current.max(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_past_two_to_the_53() {
        assert_eq!(
            compare_ids("9007199254740993", "9007199254740992"),
            Some(Ordering::Greater)
        );
        assert_eq!(compare_ids("010", "9"), Some(Ordering::Greater));
        assert_eq!(later(Some("200"), "100"), "200");
        assert_eq!(later(Some("abc"), "100"), "100");
        assert_eq!(cursor(Some("x"), Some("55")).as_deref(), Some("55"));
    }

    #[test]
    fn reading_elsewhere_lowers_unread() {
        let ids = ["100", "101", "102"];
        assert_eq!(unread_after(3, "102", ids), 0);
        assert_eq!(unread_after(3, "100", ids), 2);
        assert_eq!(unread_after(1, "100", ids), 1);
        assert_eq!(unread_after(5, "", ids), 0);
    }
}
