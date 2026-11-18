//! The registry's install and removal rules. Planning does not download or write any files.
use super::{Dependency, Index, Kind, Project, Version, dependencies::required_cycle};
use crate::dependencies::parse_version;
use mod_vocab::Side;
use semver::VersionReq;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModVersion {
    pub id: String,
    pub version: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Request {
    pub id: String,
    /// None keeps an eligible installed version or asks for the newest one that works.
    pub version: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum RequestResult {
    Installable { version: String },
    AlreadyInstalled { version: String },
    Refused { reason: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Update {
    pub id: String,
    pub from: String,
    pub to: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OptionalDependency {
    pub from: ModVersion,
    pub dependency: Dependency,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct InstallPlan {
    pub requested: BTreeMap<String, RequestResult>,
    pub install: Vec<ModVersion>,
    pub kept: Vec<ModVersion>,
    /// Already on disk and in range, but the registry has no manifest facts for this version.
    pub kept_unindexed: Vec<ModVersion>,
    pub update: Vec<Update>,
    pub optional: Vec<OptionalDependency>,
    /// Dependency id to requested ids that need it, directly or transitively.
    pub required_by: BTreeMap<String, Vec<String>>,
}

/// Resolve requested mods and their required dependencies. A connected group of requests is
/// planned together or refused together; independent groups can still be installed. Every plan
/// list is sorted by id (optional declarations by declaring id then dependency id).
///
/// `native_fits` is the caller's per-file SDK fit decision. The index only knows what build a
/// native mod was made for, not whether its used API surface is unchanged on another build.
/// `side` is the installation (client or dedicated server); `Both` considers all declarations.
/// The caller checks takedowns by hash for installed files: only it has their actual bytes.
/// A kept unindexed version has no dependency metadata to walk; the loader checks its manifest.
pub fn install(
    index: &Index,
    requests: &[Request],
    installed: &[ModVersion],
    game: &str,
    side: Side,
    native_fits: impl Fn(&Project, &Version) -> bool,
) -> InstallPlan {
    let mut plan = InstallPlan::default();
    if requests.is_empty() {
        return plan;
    }
    let mut present = BTreeMap::new();
    for m in installed {
        if let Some(previous) = present.insert(m.id.as_str(), m.version.as_str())
            && previous != m.version
        {
            let reason = format!(
                "{} has both {previous} and {} installed: keep only one version",
                m.id, m.version
            );
            plan.requested.extend(requests.iter().map(|r| {
                (
                    r.id.clone(),
                    RequestResult::Refused {
                        reason: reason.clone(),
                    },
                )
            }));
            return plan;
        }
    }
    let catalog = Catalog::new(index, game, side, native_fits, &present);
    let installed_edges: Vec<_> = present
        .iter()
        .filter_map(|(id, version)| {
            Some((
                *id,
                catalog
                    .mods
                    .get(id)?
                    .iter()
                    .find(|c| c.version == *version)?,
            ))
        })
        .collect();

    let mut pending: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for r in requests {
        pending
            .entry(&r.id)
            .or_default()
            .extend(r.version.as_deref());
    }
    // Only follow possible required edges *from* requested mods. An unrelated, unrequested
    // mod that happens to depend on two of these projects must not tie their plans together.
    let mut neighbors: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    let mut reachable = BTreeSet::new();
    let mut queue: Vec<_> = pending.keys().copied().collect();
    while let Some(id) = queue.pop() {
        if !reachable.insert(id) {
            continue;
        }
        for candidate in catalog.mods.get(id).into_iter().flatten().filter(|c| {
            c.fit == Fit::Yes
                && pending
                    .get(id)
                    .is_none_or(|pins| pins.iter().all(|v| *v == c.version))
        }) {
            for dep in &candidate.required {
                let target = dep.row.id.as_str();
                neighbors.entry(id).or_default().push(target);
                neighbors.entry(target).or_default().push(id);
                queue.push(target);
            }
        }
    }
    // A possible replacement may remove the old version's guard on another requested mod.
    // They have to be planned together, but a fixed, unrelated installed mod only adds guards.
    for &(id, candidate) in &installed_edges {
        if reachable.contains(id) {
            for target in candidate
                .required
                .iter()
                .map(|d| d.row.id.as_str())
                .filter(|target| reachable.contains(target))
            {
                neighbors.entry(id).or_default().push(target);
                neighbors.entry(target).or_default().push(id);
            }
        }
    }
    let mut seen = BTreeSet::new();
    let mut needed_by = BTreeMap::<_, BTreeSet<_>>::new();
    while let Some((&first, _)) = pending.first_key_value() {
        let mut groups = BTreeMap::new();
        let mut queue = vec![first];
        while let Some(id) = queue.pop() {
            if !seen.insert(id) {
                continue;
            }
            if let Some(pins) = pending.remove(id) {
                groups.insert(id, pins);
            }
            queue.extend(neighbors.get(id).into_iter().flatten().copied());
        }
        match catalog.solve(&groups, &present, &installed_edges) {
            Err(reason) => {
                let reason = if groups.len() > 1 {
                    let ids = groups.keys().copied().collect::<Vec<_>>().join(", ");
                    format!("{ids} cannot be installed together: {reason}")
                } else {
                    reason
                };
                plan.requested.extend(groups.keys().map(|id| {
                    (
                        (*id).to_owned(),
                        RequestResult::Refused {
                            reason: reason.clone(),
                        },
                    )
                }));
            }
            Ok(selected) => {
                for &root in groups.keys() {
                    let mut todo = vec![root];
                    let mut visited = BTreeSet::new();
                    while let Some(id) = todo.pop() {
                        if !visited.insert(id) {
                            continue;
                        }
                        if id != root {
                            needed_by
                                .entry(id.to_owned())
                                .or_default()
                                .insert(root.to_owned());
                        }
                        todo.extend(selected[id].required.iter().map(|d| d.row.id.as_str()));
                    }
                }
                for (id, candidate) in selected {
                    let version = candidate.version;
                    let row = ModVersion {
                        id: id.into(),
                        version: version.into(),
                    };
                    let outcome = match present.get(id) {
                        Some(old) if *old == version => {
                            if candidate.unindexed {
                                plan.kept_unindexed.push(row.clone());
                            } else {
                                plan.kept.push(row.clone());
                            }
                            RequestResult::AlreadyInstalled {
                                version: version.into(),
                            }
                        }
                        Some(old) => {
                            plan.update.push(Update {
                                id: id.into(),
                                from: (*old).into(),
                                to: version.into(),
                            });
                            RequestResult::Installable {
                                version: version.into(),
                            }
                        }
                        None => {
                            plan.install.push(row.clone());
                            RequestResult::Installable {
                                version: version.into(),
                            }
                        }
                    };
                    if groups.contains_key(id) {
                        plan.requested.insert(id.into(), outcome);
                    }
                    plan.optional
                        .extend(candidate.optional.iter().map(|dep| OptionalDependency {
                            from: row.clone(),
                            dependency: (*dep).clone(),
                        }));
                }
            }
        }
    }
    plan.install.sort_by(|a, b| a.id.cmp(&b.id));
    plan.kept.sort_by(|a, b| a.id.cmp(&b.id));
    plan.kept_unindexed.sort_by(|a, b| a.id.cmp(&b.id));
    plan.update.sort_by(|a, b| a.id.cmp(&b.id));
    plan.optional
        .sort_by(|a, b| (&a.from.id, &a.dependency.id).cmp(&(&b.from.id, &b.dependency.id)));
    plan.required_by = needed_by
        .into_iter()
        .map(|(id, by)| (id, by.into_iter().collect()))
        .collect();
    plan
}

/// List installed mods that directly or transitively require `id`, from the indexed metadata
/// for their exact installed versions. An optional or out-of-side dependency is not a blocker.
pub fn removal_dependents(
    index: &Index,
    installed: &[ModVersion],
    id: &str,
    side: Side,
) -> Result<Vec<ModVersion>, String> {
    let mut reverse = BTreeMap::<&str, Vec<&ModVersion>>::new();
    for m in installed {
        let Some(version) = index
            .project(&m.id)
            .filter(|p| p.kind.is_mod())
            .and_then(|p| p.versions.iter().find(|v| v.version == m.version))
        else {
            continue;
        };
        for dep in version
            .dependencies
            .iter()
            .flatten()
            .filter(|d| !d.optional && d.applies(side))
        {
            dep.range(&m.id, &m.version)?;
            reverse.entry(&dep.id).or_default().push(m);
        }
    }
    let mut seen = BTreeSet::from([id]);
    let mut queue = vec![id];
    let mut dependents = Vec::new();
    while let Some(id) = queue.pop() {
        for m in reverse.get(id).into_iter().flatten() {
            if seen.insert(&m.id) {
                queue.push(&m.id);
                dependents.push((*m).clone());
            }
        }
    }
    dependents.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(dependents)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Fit {
    Yes,
    Revoked,
    WrongGame,
}

struct Range<'a> {
    row: &'a Dependency,
    parsed: Result<VersionReq, String>,
}

struct Candidate<'a> {
    version: &'a str,
    unindexed: bool,
    number: Option<semver::Version>,
    fit: Fit,
    required: Vec<Range<'a>>,
    optional: Vec<&'a Dependency>,
    invalid_optional: Option<String>,
}

struct Catalog<'a> {
    game: &'a str,
    mods: BTreeMap<&'a str, Vec<Candidate<'a>>>,
}

type Selected<'a> = BTreeMap<&'a str, &'a Candidate<'a>>;

struct Need<'a> {
    pins: &'a [&'a str],
    ranges: Vec<(&'a str, &'a Range<'a>)>,
}

impl Need<'_> {
    fn accepts(&self, candidate: &Candidate<'_>) -> bool {
        self.pins.iter().all(|v| *v == candidate.version)
            && self.ranges.iter().all(|(_, r)| {
                r.parsed
                    .as_ref()
                    .is_ok_and(|req| candidate.number.as_ref().is_some_and(|n| req.matches(n)))
            })
    }

    fn describe(&self) -> String {
        self.pins
            .iter()
            .map(|v| format!("requested {v}"))
            .chain(
                self.ranges
                    .iter()
                    .map(|(owner, r)| format!("{owner} needs {}", r.row.version)),
            )
            .collect::<Vec<_>>()
            .join("; ")
    }
}

impl<'a> Catalog<'a> {
    fn new(
        index: &'a Index,
        game: &'a str,
        side: Side,
        native_fits: impl Fn(&Project, &Version) -> bool,
        present: &BTreeMap<&'a str, &'a str>,
    ) -> Self {
        let mut mods: BTreeMap<_, Vec<_>> = index
            .projects
            .iter()
            .filter(|p| p.kind.is_mod())
            .map(|p| {
                let mut versions: Vec<_> = p
                    .versions
                    .iter()
                    .map(|v| {
                        let mut required = Vec::new();
                        let mut optional = Vec::new();
                        let mut invalid_optional = None;
                        for dep in v.dependencies.iter().flatten().filter(|d| d.applies(side)) {
                            let parsed = dep.range(&p.id, &v.version);
                            if dep.optional {
                                if let Err(err) = parsed {
                                    invalid_optional = Some(err);
                                }
                                optional.push(dep);
                            } else {
                                required.push(Range { row: dep, parsed });
                            }
                        }
                        Candidate {
                            version: &v.version,
                            unindexed: false,
                            number: parse_version(&v.version).ok(),
                            fit: if index.revoked(&p.id, &v.file.sha256).is_some() {
                                Fit::Revoked
                            } else if !v.fits_game(game)
                                || (p.kind == Kind::NativeMod && !native_fits(p, v))
                            {
                                Fit::WrongGame
                            } else {
                                Fit::Yes
                            },
                            required,
                            optional,
                            invalid_optional,
                        }
                    })
                    .collect();
                // Equal SemVer precedence keeps the index's newest-first order.
                versions.sort_by(|a, b| match (&a.number, &b.number) {
                    (Some(a), Some(b)) => b.cmp_precedence(a),
                    _ => b.number.is_some().cmp(&a.number.is_some()),
                });
                (p.id.as_str(), versions)
            })
            .collect();
        for (&id, &version) in present {
            let candidates = mods.entry(id).or_default();
            if !candidates.iter().any(|c| c.version == version) {
                candidates.push(Candidate {
                    version,
                    unindexed: true,
                    number: parse_version(version).ok(),
                    fit: if index.revoked(id, "").is_some() {
                        Fit::Revoked
                    } else {
                        Fit::Yes
                    },
                    required: Vec::new(),
                    optional: Vec::new(),
                    invalid_optional: None,
                });
            }
        }
        Self { game, mods }
    }

    fn choices(&self, id: &str, need: &Need<'_>) -> Result<Vec<&Candidate<'a>>, String> {
        let describe = || {
            let needs = need.describe();
            if needs.is_empty() {
                id.to_owned()
            } else {
                format!("{id} ({needs})")
            }
        };
        let Some(versions) = self.mods.get(id) else {
            return Err(format!("{}: no such mod in the registry", describe()));
        };
        let mut matches = versions.iter().filter(|c| need.accepts(c)).peekable();
        if matches.peek().is_none() {
            return Err(format!(
                "{}: no listed version satisfies all requirements",
                describe()
            ));
        }
        let mut unrevoked = matches.filter(|c| c.fit != Fit::Revoked).peekable();
        if unrevoked.peek().is_none() {
            return Err(format!(
                "{}: every matching version was taken down",
                describe()
            ));
        }
        let fits: Vec<_> = unrevoked.filter(|c| c.fit == Fit::Yes).collect();
        if fits.is_empty() {
            return Err(format!(
                "{}: no matching version fits this instance (Minecraft {})",
                describe(),
                self.game
            ));
        }
        Ok(fits)
    }

    /// Depth-first search over version choices, not over mods on the call stack. New requirements
    /// can narrow an earlier choice; undo it and retry instead of keeping stale outgoing edges.
    fn solve<'s>(
        &'s self,
        roots: &BTreeMap<&'s str, Vec<&'s str>>,
        present: &BTreeMap<&str, &str>,
        installed: &[(&'s str, &'s Candidate<'a>)],
    ) -> Result<Selected<'s>, String>
    where
        'a: 's,
    {
        let mut selected = Selected::new();
        let mut stack = Vec::<(&str, std::vec::IntoIter<&Candidate<'_>>)>::new();
        // An index is external data. Bound search even if many versions combine pathologically.
        const MAX_STATES: usize = 20_000;
        let mut failure = None;
        for _ in 0..MAX_STATES {
            let step = (|| {
                let mut needs: BTreeMap<_, _> = roots
                    .iter()
                    .map(|(&id, pins)| {
                        (
                            id,
                            Need {
                                pins: pins.as_slice(),
                                ranges: Vec::new(),
                            },
                        )
                    })
                    .collect();
                for (&id, &c) in &selected {
                    if let Some(e) = &c.invalid_optional {
                        return Err(e.clone());
                    }
                    for range in &c.required {
                        range.parsed.as_ref().map_err(Clone::clone)?;
                        needs
                            .entry(range.row.id.as_str())
                            .or_insert(Need {
                                pins: &[],
                                ranges: Vec::new(),
                            })
                            .ranges
                            .push((id, range));
                    }
                }
                // An installed owner that is not being replaced still needs its own ranges.
                // Delay its guard until the owner can't be changed by a pending request; at the
                // final check these guards all apply. Never install an unrelated mod's missing deps.
                let complete = needs.keys().all(|id| selected.contains_key(id));
                if complete {
                    for &(owner, c) in installed {
                        if !selected.contains_key(owner) {
                            for range in &c.required {
                                if let Some(need) = needs.get_mut(range.row.id.as_str()) {
                                    range.parsed.as_ref().map_err(Clone::clone)?;
                                    need.ranges.push((owner, range));
                                }
                            }
                        }
                    }
                }
                for (&id, &chosen) in &selected {
                    if let Some(need) = needs.get(id)
                        && !need.accepts(chosen)
                    {
                        // Distinguish a true disjoint range from a choice another branch can undo.
                        self.choices(id, need)?;
                        return Err(format!(
                            "{id} {} does not fit all requirements ({})",
                            chosen.version,
                            need.describe()
                        ));
                    }
                }
                let next = roots
                    .keys()
                    .chain(needs.keys())
                    .find(|id| !selected.contains_key(**id));
                if let Some(&id) = next {
                    let mut choices = self.choices(id, &needs[id])?;
                    if let Some(at) = choices
                        .iter()
                        .position(|c| present.get(id).is_some_and(|v| *v == c.version))
                    {
                        choices[..=at].rotate_right(1);
                    }
                    return Ok(Some((id, choices.into_iter())));
                }
                let nodes: Vec<_> = selected.keys().copied().collect();
                let edges: Vec<_> = selected
                    .values()
                    .map(|c| {
                        c.required
                            .iter()
                            .filter_map(|r| nodes.binary_search(&r.row.id.as_str()).ok())
                            .collect()
                    })
                    .collect();
                if let Some(cycle) = required_cycle(&edges) {
                    let names: Vec<_> = cycle.into_iter().map(|i| nodes[i]).collect();
                    return Err(format!("required dependency cycle: {}", names.join(" -> ")));
                }
                Ok(None)
            })();
            match step {
                Ok(None) => return Ok(selected),
                Ok(Some(frame)) => stack.push(frame),
                Err(reason) => {
                    failure.get_or_insert(reason);
                }
            }
            while let Some((id, choices)) = stack.last_mut() {
                if let Some(next) = choices.next() {
                    selected.insert(*id, next);
                    break;
                }
                selected.remove(id);
                stack.pop();
            }
            if stack.is_empty() {
                return Err(failure
                    .unwrap_or_else(|| "no consistent versions for the requested mods".into()));
            }
        }
        Err(format!(
            "{}: dependency resolution limit reached; try fewer requests or pin a version",
            roots.keys().copied().collect::<Vec<_>>().join(", ")
        ))
    }
}
