//! Checks for the index publisher. Readers keep entries with unavailable dependencies so the
//! installer can explain why a requested version cannot be installed.
use super::{Dependency, Index, Kind};
use crate::dependencies::parse_version;
use mod_vocab::Side;
use semver::{Comparator, Op, Version, VersionReq};
use std::collections::{BTreeMap, HashSet};

impl Dependency {
    pub(super) fn range(&self, owner: &str, version: &str) -> Result<VersionReq, String> {
        VersionReq::parse(&self.version).map_err(|_| {
            format!(
                "{owner} {version} declares {} {}: use a semver range in sunrise-mod.toml and rebuild the index",
                self.id, self.version
            )
        })
    }

    pub(super) fn applies(&self, side: Side) -> bool {
        side == Side::Both || self.side.is_none_or(|s| s == Side::Both || s == side)
    }
}

impl Kind {
    pub(super) fn is_mod(&self) -> bool {
        matches!(self, Self::Mod | Self::NativeMod)
    }
}

impl Index {
    pub(super) fn check_dependencies(&self) -> Result<(), String> {
        let mut by_id: BTreeMap<&str, Vec<_>> = BTreeMap::new();
        let versions: Vec<_> = self
            .projects
            .iter()
            .flat_map(|p| p.versions.iter().map(move |v| (p, v)))
            .collect();
        for (i, (p, v)) in versions.iter().enumerate() {
            if p.kind.is_mod() {
                by_id
                    .entry(&p.id)
                    .or_default()
                    .push((i, parse_version(&v.version).ok()));
            }
        }
        let mut edges = vec![Vec::new(); versions.len()];
        for (from, (p, v)) in versions.iter().enumerate() {
            let dependencies = v.dependencies.as_deref().unwrap_or_default();
            if !p.kind.is_mod() && !dependencies.is_empty() {
                return Err(format!(
                    "{} {} lists dependency {}: only mods can declare dependencies; rebuild the index from the package manifests",
                    p.id, v.version, dependencies[0].id
                ));
            }
            let mut seen = HashSet::new();
            for dep in dependencies {
                if !super::valid_id(&dep.id) || !seen.insert(&dep.id) {
                    return Err(format!(
                        "{} {} declares {} {}: use a valid, unique mod id in sunrise-mod.toml",
                        p.id, v.version, dep.id, dep.version
                    ));
                }
                let range = dep.range(&p.id, &v.version)?;
                if dep.optional {
                    continue;
                }
                let needs = format!("{} {} requires {} {}", p.id, v.version, dep.id, dep.version);
                let Some(candidates) = by_id.get(dep.id.as_str()) else {
                    return Err(format!(
                        "{needs}, which the registry does not list as a mod: list it first, or mark it optional"
                    ));
                };
                let before = edges[from].len();
                edges[from].extend(candidates.iter().filter_map(|(to, number)| {
                    number
                        .as_ref()
                        .filter(|n| range.matches(n))
                        .map(|_| (*to, dep))
                }));
                if edges[from].len() != before {
                    continue;
                }
                if let Some((_, Some(listed))) = candidates.first()
                    && candidates.iter().all(|(_, number)| {
                        number
                            .as_ref()
                            .is_some_and(|n| excluded_as_prerelease(&range, n))
                    })
                {
                    return Err(format!(
                        "{needs}, which excludes the registry's prereleases: use a range that names the prerelease, such as ={listed}"
                    ));
                }
                return Err(format!(
                    "{needs}, but no listed version matches: list a matching version, or correct the range in sunrise-mod.toml"
                ));
            }
        }
        for side in [Side::Client, Side::Server] {
            let graph: Vec<Vec<usize>> = edges
                .iter()
                .map(|row| {
                    row.iter()
                        .filter_map(|(to, dep)| dep.applies(side).then_some(*to))
                        .collect()
                })
                .collect();
            if let Some(cycle) = required_cycle(&graph) {
                let names: Vec<_> = cycle
                    .into_iter()
                    .map(|i| {
                        let (p, v) = versions[i];
                        format!("{} {}", p.id, v.version)
                    })
                    .collect();
                return Err(format!(
                    "required dependency cycle ({}): change a dependency in sunrise-mod.toml to break it",
                    names.join(" -> ")
                ));
            }
        }
        Ok(())
    }
}

/// Test the same comparators with an extra exact comparator to opt in to the prerelease. If it
/// then matches, SemVer's prerelease opt-in (rather than the numeric bounds) was the only block.
fn excluded_as_prerelease(range: &VersionReq, version: &Version) -> bool {
    if version.pre.is_empty() {
        return false;
    }
    let mut opted_in = range.clone();
    opted_in.comparators.push(Comparator {
        op: Op::Exact,
        major: version.major,
        minor: Some(version.minor),
        patch: Some(version.patch),
        pre: version.pre.clone(),
    });
    opted_in.matches(version)
}

/// Iterative depth-first traversal: index size and graph depth are input, not stack limits.
pub(super) fn required_cycle(edges: &[Vec<usize>]) -> Option<Vec<usize>> {
    enum Mark {
        New,
        Visiting(usize),
        Done,
    }
    let mut marks: Vec<_> = (0..edges.len()).map(|_| Mark::New).collect();
    let mut stack = Vec::new();
    for start in 0..edges.len() {
        if !matches!(marks[start], Mark::New) {
            continue;
        }
        marks[start] = Mark::Visiting(0);
        stack.push((start, edges[start].iter()));
        while let Some((node, next)) = stack.last_mut() {
            match next.next().copied() {
                None => {
                    marks[*node] = Mark::Done;
                    stack.pop();
                }
                Some(to) => match marks[to] {
                    Mark::New => {
                        marks[to] = Mark::Visiting(stack.len());
                        stack.push((to, edges[to].iter()));
                    }
                    Mark::Done => {}
                    Mark::Visiting(at) => {
                        return Some(stack[at..].iter().map(|(i, _)| *i).chain([to]).collect());
                    }
                },
            }
        }
    }
    None
}
