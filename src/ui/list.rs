//! Repo list: filtered, sorted, themed rows.

use crate::core::models::{Repo, RepoStatus};
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;

/// Sort keys for the list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortKey {
    Name,
    NewestRelease,
    LastChecked,
}

/// Compare two repos under a sort key. Pinned always sorts first.
/// Pure — unit-tested.
pub fn compare(a: &Repo, b: &Repo, key: SortKey) -> std::cmp::Ordering {
    use std::cmp::Ordering as O;
    match (a.pinned, b.pinned) {
        (true, false) => return O::Less,
        (false, true) => return O::Greater,
        _ => {}
    }
    match key {
        SortKey::Name => a.display_name().to_lowercase().cmp(&b.display_name().to_lowercase()),
        SortKey::NewestRelease => {
            // Newest first: compare published_at descending, missing last.
            match (&a.latest, &b.latest) {
                (None, None) => O::Equal,
                (None, Some(_)) => O::Greater,
                (Some(_), None) => O::Less,
                (Some(x), Some(y)) => y.published_at.cmp(&x.published_at),
            }
        }
        SortKey::LastChecked => O::Equal, // Task 9 tracks last_checked timestamps
    }
}

/// Fuzzy filter over name+owner+category. Empty query keeps all.
/// Pure — unit-tested.
pub fn matches_query(repo: &Repo, query: &str) -> bool {
    let q = query.trim();
    if q.is_empty() {
        return true;
    }
    let matcher = SkimMatcherV2::default();
    let haystacks = [
        repo.name.clone(),
        repo.owner.clone(),
        repo.category.clone().unwrap_or_default(),
        repo.display_name(),
    ];
    haystacks
        .iter()
        .any(|h| matcher.fuzzy_match(h, q).is_some())
}

/// Category → ribbon color (8 fixed slots, hash-stable per category name).
pub fn ribbon_color(category: Option<&str>) -> egui::Color32 {
    match category {
        None => crate::ui::theme::Tokens::BORDER,
        Some(c) => {
            let idx = c.bytes().map(|b| b as usize).sum::<usize>() % crate::ui::theme::Tokens::RIBBONS.len();
            crate::ui::theme::Tokens::RIBBONS[idx]
        }
    }
}

/// Asset-name glob matching for the per-repo asset pattern filter.
/// Pattern `""`/None matches nothing (no marks). Pure — unit-tested.
pub fn asset_matches(pattern: Option<&str>, asset_name: &str) -> bool {
    use globset::{Glob, GlobSetBuilder};
    let Some(p) = pattern.map(str::trim).filter(|p| !p.is_empty()) else {
        return false;
    };
    let glob = match Glob::new(p) {
        Ok(g) => g,
        Err(_) => return false, // invalid pattern marks nothing
    };
    let set = match GlobSetBuilder::new().add(glob).build() {
        Ok(s) => s,
        Err(_) => return false,
    };
    set.is_match(asset_name)
}

/// Sort assets: pattern-matching first (stable within groups, original
/// order otherwise). Pure — unit-tested. Detail view sorts via
/// sort_by_key inline; this helper stays as the tested reference.
#[allow(dead_code)]
pub fn sort_assets<'a>(
    assets: &[&'a crate::core::models::Asset],
    pattern: Option<&str>,
) -> Vec<&'a crate::core::models::Asset> {
    let mut out: Vec<&crate::core::models::Asset> = assets.to_vec();
    out.sort_by_key(|a| !asset_matches(pattern, &a.name));
    out
}

/// Human summary of a repo status for the status pill.
pub fn status_label(status: &RepoStatus) -> String {
    match status {
        RepoStatus::Ok => "OK".into(),
        RepoStatus::NoStableRelease => "no stable release".into(),
        RepoStatus::NoReleases => "no releases".into(),
        RepoStatus::Error(m) => format!("error: {m}"),
        RepoStatus::RateLimited => "rate limited".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::models::{Asset, Release, ReleaseFilters, RepoHost};

    fn repo(name: &str, owner: &str, pinned: bool, published: Option<&str>) -> Repo {
        Repo {
            id: format!("github.com/{owner}/{name}"),
            host: RepoHost::GitHub,
            owner: owner.into(),
            name: name.into(),
            category: None,
            pinned,
            filters: ReleaseFilters::default(),
            gl_project_id: None,
            last_seen_tag: None,
            latest: published.map(|at| Release {
                tag: "1.0.0".into(),
                published_at: at.into(),
                prerelease: false,
                notes: String::new(),
                assets: vec![Asset {
                    name: "x.zip".into(),
                    size: 1,
                    url: "https://x".into(),
                }],
            }),
            status: RepoStatus::Ok,
            new_release: false,
        }
    }

    #[test]
    fn pinned_sorts_first_regardless_of_key() {
        let a = repo("aaa", "o", false, Some("2026-11-01T00:00:00Z"));
        let p = repo("zzz", "o", true, Some("2026-01-01T00:00:00Z"));
        assert_eq!(compare(&p, &a, SortKey::Name), std::cmp::Ordering::Less);
        assert_eq!(compare(&p, &a, SortKey::NewestRelease), std::cmp::Ordering::Less);
    }

    #[test]
    fn name_sort_is_case_insensitive() {
        let a = repo("apple", "o", false, None);
        let b = repo("Banana", "o", false, None);
        assert_eq!(compare(&a, &b, SortKey::Name), std::cmp::Ordering::Less);
    }

    #[test]
    fn newest_release_sort_descending_with_missing_last() {
        let old = repo("a", "o", false, Some("2026-01-01T00:00:00Z"));
        let new = repo("b", "o", false, Some("2026-11-01T00:00:00Z"));
        let none = repo("c", "o", false, None);
        assert_eq!(compare(&new, &old, SortKey::NewestRelease), std::cmp::Ordering::Less);
        assert_eq!(compare(&new, &none, SortKey::NewestRelease), std::cmp::Ordering::Less);
        assert_eq!(compare(&none, &old, SortKey::NewestRelease), std::cmp::Ordering::Greater);
    }

    #[test]
    fn fuzzy_matches_name_owner_category() {
        let mut r = repo("serde", "serde-rs", false, None);
        assert!(matches_query(&r, "ser"));
        assert!(matches_query(&r, "serde-rs")); // owner
        r.category = Some("networking".into());
        assert!(matches_query(&r, "netwrkng")); // fuzzy on category
        assert!(!matches_query(&r, "zzz"));
        assert!(matches_query(&r, "")); // empty keeps all
        assert!(matches_query(&r, "   ")); // whitespace keeps all
    }

    #[test]
    fn ribbon_color_stable_per_category() {
        assert_eq!(ribbon_color(Some("tools")), ribbon_color(Some("tools")));
        assert_ne!(ribbon_color(Some("tools")), ribbon_color(Some("media")));
        assert_eq!(ribbon_color(None), crate::ui::theme::Tokens::BORDER);
    }

    #[test]
    fn asset_pattern_matches_globs() {
        assert!(asset_matches(Some("*-x86_64.apk"), "app-1.2-x86_64.apk"));
        assert!(!asset_matches(Some("*-x86_64.apk"), "app-1.2-arm64.apk"));
        assert!(asset_matches(Some("*.zip"), "src.zip"));
        assert!(!asset_matches(Some("*.zip"), "src.tar.gz"));
    }

    #[test]
    fn asset_pattern_empty_or_invalid_matches_nothing() {
        assert!(!asset_matches(None, "anything.zip"));
        assert!(!asset_matches(Some(""), "anything.zip"));
        assert!(!asset_matches(Some("   "), "anything.zip"));
        // Invalid glob (unclosed bracket) marks nothing, never panics.
        assert!(!asset_matches(Some("[unclosed"), "a.zip"));
    }

    #[test]
    fn sort_assets_puts_matches_first_stably() {
        use crate::core::models::Asset;
        let mk = |n: &str| Asset { name: n.into(), size: 0, url: String::new() };
        let a = mk("zz.zip");
        let b = mk("match-x86_64.apk");
        let c = mk("aa.zip");
        let assets = vec![&a, &b, &c];
        let sorted = sort_assets(&assets, Some("*x86_64*"));
        assert_eq!(sorted[0].name, "match-x86_64.apk");
        // Non-matches keep original relative order (stable).
        assert_eq!(sorted[1].name, "zz.zip");
        assert_eq!(sorted[2].name, "aa.zip");
    }

    #[test]
    fn sort_assets_no_pattern_keeps_order() {
        use crate::core::models::Asset;
        let mk = |n: &str| Asset { name: n.into(), size: 0, url: String::new() };
        let a = mk("zz.zip");
        let b = mk("aa.zip");
        let assets = vec![&a, &b];
        let sorted = sort_assets(&assets, None);
        assert_eq!(sorted[0].name, "zz.zip");
        assert_eq!(sorted[1].name, "aa.zip");
    }
}
