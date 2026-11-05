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
}
