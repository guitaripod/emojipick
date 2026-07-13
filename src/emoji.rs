use crate::frecency::Frecency;
use emojis::{Emoji, Group, SkinTone};
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use once_cell::sync::Lazy;

#[derive(Debug, Clone)]
pub struct Item {
    pub glyph: String,
    pub base: String,
    pub name: String,
    pub shortcode: String,
}

pub const CATEGORIES: &[(&str, Group)] = &[
    ("Smileys", Group::SmileysAndEmotion),
    ("People", Group::PeopleAndBody),
    ("Nature", Group::AnimalsAndNature),
    ("Food", Group::FoodAndDrink),
    ("Travel", Group::TravelAndPlaces),
    ("Activities", Group::Activities),
    ("Objects", Group::Objects),
    ("Symbols", Group::Symbols),
    ("Flags", Group::Flags),
];

struct CatalogEntry {
    e: &'static Emoji,
    name_lower: String,
    shortcodes: Vec<&'static str>,
}

static MATCHER: Lazy<SkimMatcherV2> = Lazy::new(SkimMatcherV2::default);

static CATALOG: Lazy<Vec<CatalogEntry>> = Lazy::new(|| {
    emojis::iter()
        .filter(|e| e.skin_tone().map_or(true, |t| t == SkinTone::Default))
        .map(|e| CatalogEntry {
            e,
            name_lower: e.name().to_lowercase(),
            shortcodes: e.shortcodes().collect(),
        })
        .collect()
});

fn tone_enum(tone: u8) -> Option<SkinTone> {
    Some(match tone {
        1 => SkinTone::Light,
        2 => SkinTone::MediumLight,
        3 => SkinTone::Medium,
        4 => SkinTone::MediumDark,
        5 => SkinTone::Dark,
        _ => return None,
    })
}

pub fn apply_skin_tone(e: &'static Emoji, tone: u8) -> String {
    match tone_enum(tone) {
        Some(st) => e
            .with_skin_tone(st)
            .map(|v| v.as_str())
            .unwrap_or_else(|| e.as_str())
            .to_string(),
        None => e.as_str().to_string(),
    }
}

fn short_display(shortcodes: &[&'static str]) -> String {
    shortcodes
        .first()
        .map(|s| format!(":{s}:"))
        .unwrap_or_default()
}

fn to_item(entry: &CatalogEntry, tone: u8) -> Item {
    Item {
        glyph: apply_skin_tone(entry.e, tone),
        base: entry.e.as_str().to_string(),
        name: entry.e.name().to_string(),
        shortcode: short_display(&entry.shortcodes),
    }
}

/// Resolve a stored (base, default-tone) glyph into a display Item under the active tone.
pub fn item_for_glyph(base: &str, tone: u8) -> Option<Item> {
    let e = emojis::get(base)?;
    let e = e
        .skin_tone()
        .and_then(|_| e.with_skin_tone(SkinTone::Default))
        .unwrap_or(e);
    Some(Item {
        glyph: apply_skin_tone(e, tone),
        base: e.as_str().to_string(),
        name: e.name().to_string(),
        shortcode: short_display(&e.shortcodes().collect::<Vec<_>>()),
    })
}

pub fn all(tone: u8) -> Vec<Item> {
    CATALOG.iter().map(|c| to_item(c, tone)).collect()
}

pub fn by_group(group: Group, tone: u8) -> Vec<Item> {
    CATALOG
        .iter()
        .filter(|c| c.e.group() == group)
        .map(|c| to_item(c, tone))
        .collect()
}

fn tier_for(entry: &CatalogEntry, q: &str) -> Option<(u8, i64)> {
    let name = &entry.name_lower;
    if name == q || entry.shortcodes.iter().any(|s| *s == q) {
        return Some((0, i64::MAX));
    }
    if name.starts_with(q) || entry.shortcodes.iter().any(|s| s.starts_with(q)) {
        return Some((1, i64::MAX));
    }
    if name.split(&[' ', '_', '-'][..]).any(|w| w.starts_with(q)) {
        return Some((2, i64::MAX));
    }
    let name_score = MATCHER.fuzzy_match(name, q);
    let code_score = entry
        .shortcodes
        .iter()
        .filter_map(|s| MATCHER.fuzzy_match(s, q))
        .max();
    let score = name_score.max(code_score)?;
    Some((3, score))
}

pub fn search(query: &str, tone: u8, frecency: &Frecency) -> Vec<Item> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return all(tone);
    }

    let mut scored: Vec<(u8, i64, f64, &CatalogEntry)> = CATALOG
        .iter()
        .filter_map(|entry| {
            let (tier, fuzzy) = tier_for(entry, &q)?;
            let frec = frecency.score(&entry.e.as_str().to_string());
            let effective = if frec > 2.0 && tier > 1 { tier - 1 } else { tier };
            Some((effective, fuzzy, frec, entry))
        })
        .collect();

    scored.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then(b.1.cmp(&a.1))
            .then(b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal))
    });

    let mut seen = std::collections::HashSet::new();
    scored
        .into_iter()
        .map(|(_, _, _, entry)| to_item(entry, tone))
        .filter(|item| seen.insert(item.glyph.clone()))
        .collect()
}
