use super::*;
use crate::index::{
    Index,
    registry::{
        self,
        tests::{Scratch, png},
    },
    sha256,
};
use std::io::{Cursor, Write};

const ID: &str = "sunrise_creatures";
const COLLECTION: &str = r#"id = "sunrise_creatures"
name = "Sunrise Creatures"
summary = "Fireflies and butterflies with the library they share."
owners = ["EAnathos"]
icon = "icons/sunrise_creatures.png"
members = [
  { id = "creature" },
  { id = "fireflies" },
  { id = "butterflies", default = false },
]
"#;

fn write_mod(s: &Scratch, id: &str, name: &str, owner: &str) {
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    zip.start_file(
        crate::manifest::MANIFEST,
        zip::write::SimpleFileOptions::default(),
    )
    .unwrap();
    write!(
        zip,
        "id='{id}'\nname='{name}'\nversion='1.0.0'\napi='0.2'\ncapabilities=['registries']\n"
    )
    .unwrap();
    let bytes = zip.finish().unwrap().into_inner();
    let hash = sha256(&bytes);
    std::fs::write(s.0.join("files").join(&hash), &bytes).unwrap();
    s.write_bytes(&format!("icons/{id}.png"), &png(128, 128));
    s.write(
        &format!("mods/{id}.toml"),
        &format!(
            r#"
id = "{id}"
kind = "mod"
name = "{name}"
summary = "An example mod for collection validation."
description = "An example content mod, for testing the registry's collection rules."
owners = ["{owner}"]
license = "MIT"
source = "https://example.test/{id}"
icon = "icons/{id}.png"
categories = ["utility"]
[[versions]]
version = "1.0.0"
published = "2026-11-18"
game = "26.3"
file = "{id}.smod"
sha256 = "{hash}"
"#
        ),
    );
}

fn checkout(name: &str) -> Scratch {
    let s = Scratch::new(name);
    write_mod(&s, "creature", "Sunrise Creatures", "EAnathos");
    write_mod(&s, "fireflies", "Fireflies", "EAnathos");
    write_mod(&s, "butterflies", "Butterflies", "EAnathos");
    s.write_bytes("collections/sunrise_creatures.toml", COLLECTION.as_bytes());
    s.write_bytes("icons/sunrise_creatures.png", &png(128, 128));
    s
}

#[test]
fn a_collection_and_project_can_have_the_same_name_but_have_distinct_ids() {
    let s = checkout("collection-format");
    let index = s.build().unwrap();
    let collection = index.collection(ID).unwrap();
    assert_eq!(collection.name, index.project("creature").unwrap().name);
    assert!(collection.icon.contains("icons/sunrise_creatures.png?v="));
    let json = serde_json::to_value(collection).unwrap();
    assert_eq!(
        json["members"],
        serde_json::json!([
            {"id": "creature"}, {"id": "fireflies"}, {"id": "butterflies", "default": false}
        ])
    );
    for absent in ["owner_ids", "file", "versions"] {
        assert!(json.get(absent).is_none());
    }
    let text = serde_json::to_string(&index).unwrap();
    assert_eq!(Index::parse(&text).unwrap(), index);
    let mut index = index;
    index.projects.retain(|p| p.id != "creature");
    let read = Index::parse(&serde_json::to_string(&index).unwrap()).unwrap();
    assert!(read.collection(ID).is_none());
    assert!(read.project("fireflies").is_some());
    assert!(
        read.left_out
            .iter()
            .any(|row| row.id == ID && row.reason.contains("member creature"))
    );
}

#[test]
fn collection_entries_are_bounded_regular_toml_named_by_their_ids() {
    let s = checkout("collection-loader");
    s.write("collections/wrong.toml", COLLECTION);
    s.write_bytes(
        "collections/large.toml",
        &vec![b' '; (registry::MAX_COLLECTION_BYTES + 1) as usize],
    );
    std::fs::create_dir(s.0.join("repo/collections/dir.toml")).unwrap();
    let errors = registry::load(&s.0.join("repo")).unwrap_err();
    for (file, problem) in [
        ("wrong.toml", "sunrise_creatures.toml"),
        ("large.toml", "byte limit"),
        ("dir.toml", "not a regular file"),
    ] {
        assert!(
            errors
                .iter()
                .any(|e| e.contains(file) && e.contains(problem)),
            "{errors:?}"
        );
    }
}

#[test]
fn the_publisher_refuses_invalid_collection_links_and_selection() {
    let s = checkout("collection-check");
    let index = s.build().unwrap();
    let valid = index.collection(ID).unwrap();
    let check = |collection: Collection, diagnosis: &str| {
        let mut proposed = index.clone();
        let id = collection.id.clone();
        proposed.collections = vec![collection];
        let error = proposed.check().unwrap_err();
        assert!(error.contains(&id) && error.contains(diagnosis), "{error}");
    };
    check(
        Collection {
            id: "creature".into(),
            ..valid.clone()
        },
        "already a project id",
    );
    check(
        Collection {
            id: "submit".into(),
            ..valid.clone()
        },
        "non-reserved",
    );
    let one = valid.members[..1].to_vec();
    check(
        Collection {
            members: one.clone(),
            ..valid.clone()
        },
        "choose 2 to",
    );
    check(
        Collection {
            members: vec![valid.members[0].clone(); crate::index::MAX_COLLECTION_MEMBERS + 1],
            ..valid.clone()
        },
        "choose 2 to",
    );
    let mut row = valid.clone();
    row.members.push(row.members[0].clone());
    check(row, "member creature is listed twice");
    let mut row = valid.clone();
    row.members[0].id = "cozy".into(); // A modpack, not a member project.
    check(row, "member cozy is not a listed mod");
    let mut row = valid.clone();
    row.members[0].id = ID.into(); // Collections cannot be members, including self.
    check(row, "member sunrise_creatures is not a listed mod");
    let mut row = valid.clone();
    row.members.iter_mut().for_each(|m| m.default = false);
    check(row, "choose at least one member installed by default");

    // The lightweight review detects the removal without package downloads too.
    let root = s.0.join("repo");
    std::fs::remove_file(root.join("mods/creature.toml")).unwrap();
    let errors = registry::load(&root).unwrap().check_collections(&Sources {
        registry: &root,
        files: &root,
        release: crate::index::FILE_HOST,
        site: "https://example.test/",
        generated: String::new(),
    });
    assert!(
        errors
            .iter()
            .any(|e| e.contains(ID) && e.contains("member creature is not a listed mod")),
        "{errors:?}"
    );
}

#[test]
fn a_takedown_lists_collections_using_its_project() {
    let s = checkout("collection-takedown");
    let mut index = s.build().unwrap();
    let base = registry::load(&s.0.join("repo")).unwrap();
    index.revoked.push(crate::index::Revoked {
        id: "creature".into(),
        sha256: Some(
            index.project("creature").unwrap().versions[0]
                .file
                .sha256
                .clone(),
        ),
        reason: "test".into(),
        date: String::new(),
    });
    let notes = registry::takedown_notes(&base.entries, &base.revoked, &index);
    assert!(
        notes
            .iter()
            .any(|n| n.contains(ID) && n.contains("creature") && n.contains("takedown")),
        "{notes:?}"
    );
}

#[test]
fn only_owners_membership_edits_for_their_projects_are_routine() {
    let s = checkout("collection-owner");
    write_mod(&s, "cyclops", "Cyclops", "EAnathos");
    write_mod(&s, "vulture", "Vulture", "someone_else");
    let root = s.0.join("repo");
    let base = registry::load(&root).unwrap();
    let owner = "eanathos";
    let review = |edit: &dyn Fn(&mut CollectionEntry), who: &str| {
        let mut head = registry::load(&root).unwrap();
        edit(&mut head.collections[0]);
        let changes = review_collections(&base, &head, who);
        assert_eq!(changes.len(), 1);
        changes.into_iter().next().unwrap()
    };
    let add = |id: &str| {
        let id = id.to_owned();
        move |c: &mut CollectionEntry| {
            c.members.push(Member {
                id: id.clone(),
                default: false,
            })
        }
    };
    let remove = |c: &mut CollectionEntry| {
        c.members.retain(|m| m.id != "butterflies");
    };
    assert_eq!(
        review(&add("cyclops"), owner).status,
        CollectionReviewStatus::Routine
    );
    assert_eq!(
        review(&remove, owner).status,
        CollectionReviewStatus::Routine
    );
    let foreign = review(&add("vulture"), owner);
    assert_eq!(foreign.status, CollectionReviewStatus::ReviewRequired);
    assert!(foreign.reason.contains("vulture"));
    let non_owner = review(&add("cyclops"), "someone_else");
    assert_eq!(non_owner.status, CollectionReviewStatus::ReviewRequired);
    assert!(non_owner.reason.contains("someone_else"));
    for edit in [
        (|c: &mut CollectionEntry| c.members[0].default = false) as fn(&mut CollectionEntry),
        |c: &mut CollectionEntry| c.name = "Other title".into(),
        |c: &mut CollectionEntry| c.owners.push("new_owner".into()),
    ] {
        assert_eq!(
            review(&edit, owner).status,
            CollectionReviewStatus::ReviewRequired
        );
    }
}

#[test]
fn replacing_an_icon_at_the_same_path_requires_review() {
    let base = checkout("collection-icon-old");
    let head = checkout("collection-icon-new");
    head.write_bytes("icons/sunrise_creatures.png", &png(256, 256));
    let base = registry::load(&base.0.join("repo")).unwrap();
    let head = registry::load(&head.0.join("repo")).unwrap();
    let changes = review_collections(&base, &head, "EAnathos");
    assert_eq!(changes[0].status, CollectionReviewStatus::ReviewRequired);
    assert!(changes[0].reason.contains("review the page"));
}
