//! Concrete authoring extracted verbatim from the vocabulary layer.
use super::*;

impl RecipeBooksRegistrar for PartRegistrar {
    fn recipe_book_type(&mut self, id: &str) -> Result<RecipeBookTypeKey, String> {
        self.register_book(id, None)
    }

    fn recipe_book_type_with_fields(
        &mut self,
        id: &str,
        open_field: &str,
        filtering_field: &str,
    ) -> Result<RecipeBookTypeKey, String> {
        self.register_book(id, Some((open_field, filtering_field)))
    }
}

impl PartRegistrar {
    fn register_book(
        &mut self,
        id: &str,
        fields: Option<(&str, &str)>,
    ) -> Result<RecipeBookTypeKey, String> {
        let result = (|| {
            if mod_vocab::__private::parts_installation::parts_installed() {
                return Err("registry is frozen".to_owned());
            }
            let (namespace, path) = id.split_once(':').ok_or("explicit namespace required")?;
            if id.len() > 256 {
                return Err("id exceeds 256 bytes".into());
            }
            if namespace.is_empty()
                || path.is_empty()
                || !namespace
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"_.-".contains(&b))
                || !path
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"_.-/".contains(&b))
            {
                return Err("nonempty lowercase resource-id namespace and path required".into());
            }
            if let Some(owner) = self.recipe_book_types.get(id) {
                return Err(format!(
                    "duplicate key declared by {owner} and {}",
                    self.mod_id
                ));
            }
            if namespace == "minecraft" || !owns_namespace(self.mod_id, namespace) {
                return Err("namespace must be owned by the registering mod, not minecraft".into());
            }
            if self.recipe_book_types.len() >= 1024 {
                return Err("registry exceeds 1024 custom books".into());
            }
            if let Some((open, filtering)) = fields {
                const RESERVED: &[&str] = &[
                    "isGuiOpen",
                    "isFilteringCraftable",
                    "isFurnaceGuiOpen",
                    "isFurnaceFilteringCraftable",
                    "isBlastingFurnaceGuiOpen",
                    "isBlastingFurnaceFilteringCraftable",
                    "isSmokerGuiOpen",
                    "isSmokerFilteringCraftable",
                    "recipes",
                    "toBeDisplayed",
                    "sunrise:book_settings",
                ];
                if open == filtering {
                    return Err("open and filtering fields must differ".into());
                }
                for field in [open, filtering] {
                    if field.is_empty() || field.len() > 256 {
                        return Err("save fields must be nonempty and at most 256 bytes".into());
                    }
                    if RESERVED.contains(&field) {
                        return Err(format!("reserved save field {field:?}"));
                    }
                    if let Some((other, _)) = self
                        .recipe_book_fields
                        .iter()
                        .find(|(_, (a, b))| field == a || field == b)
                    {
                        return Err(format!(
                            "save field {field:?} already belongs to book {other:?}"
                        ));
                    }
                }
                self.recipe_book_fields
                    .insert(id.into(), (open.into(), filtering.into()));
            }
            self.recipe_book_types.insert(id.into(), self.mod_id.into());
            Ok(mod_vocab::__private::parts_installation::recipe_book_type(
                id.into(),
            ))
        })();
        result.map_err(|reason: String| {
            let message = format!("mod {} recipe book {id:?}: {reason}", self.mod_id);
            self.refused.push(message.clone());
            message
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flat_fields_are_validated_atomically_and_refuse_installation() {
        let mut r = PartRegistrar::new("demo");
        r.recipe_book_type_with_fields(
            "demo:pot",
            "isModCookingGuiOpen",
            "isModCookingFilteringCraftable",
        )
        .unwrap();
        r.recipe_book_type_with_fields("demo:unicode", "Ouvert é / !", "絞り込み")
            .unwrap();
        let reserved = [
            "isGuiOpen",
            "isFilteringCraftable",
            "isFurnaceGuiOpen",
            "isFurnaceFilteringCraftable",
            "isBlastingFurnaceGuiOpen",
            "isBlastingFurnaceFilteringCraftable",
            "isSmokerGuiOpen",
            "isSmokerFilteringCraftable",
            "recipes",
            "toBeDisplayed",
            "sunrise:book_settings",
        ];
        for field in reserved {
            assert!(
                r.recipe_book_type_with_fields("demo:bad", field, "unused")
                    .is_err()
            );
            assert!(
                r.recipe_book_type_with_fields("demo:bad", "unused", field)
                    .is_err()
            );
        }
        for (a, b) in [("", "valid"), ("valid", ""), ("equal", "equal")] {
            assert!(r.recipe_book_type_with_fields("demo:bad", a, b).is_err());
        }
        assert!(
            r.recipe_book_type_with_fields("demo:bad", &"é".repeat(129), "unused")
                .is_err()
        );
        r.mod_id = "second";
        for field in ["isModCookingGuiOpen", "isModCookingFilteringCraftable"] {
            assert!(
                r.recipe_book_type_with_fields("second:bad", field, "unused")
                    .is_err()
            );
            assert!(
                r.recipe_book_type_with_fields("second:bad", "unused", field)
                    .is_err()
            );
        }
        assert!(!r.refused.is_empty(), "ignored errors refuse installation");
        assert_eq!(r.recipe_book_fields.len(), 2);
        assert_eq!(r.recipe_book_types.len(), 2);
        r.recipe_book_type_with_fields("second:ok", "unused", &"é".repeat(128))
            .unwrap();
    }

    #[test]
    fn registration_rejects_invalid_and_duplicate_declarations() {
        let mut registrar = PartRegistrar::new("demo");
        let key = registrar.recipe_book_type("demo:cooking_pot").unwrap();
        assert_eq!(key.as_str(), "demo:cooking_pot");
        assert!(
            registrar
                .recipe_book_type("demo:cooking_pot")
                .unwrap_err()
                .contains("duplicate")
        );
        for id in [
            "pot",
            "demo:",
            ":pot",
            "demo:Pot",
            "minecraft:pot",
            "other:pot",
            "demo:a:b",
        ] {
            assert!(registrar.recipe_book_type(id).is_err(), "{id}");
        }
        assert!(
            registrar
                .recipe_book_type(&format!("demo:{}", "a".repeat(252)))
                .is_err()
        );
        assert_eq!(registrar.recipe_book_types.len(), 1);
        assert_eq!(registrar.refused.len(), 9);
    }

    #[test]
    fn cross_mod_duplicate_names_both_declarers() {
        let mut registrar = PartRegistrar::new("first");
        registrar.recipe_book_type("first:book").unwrap();
        registrar.mod_id = "second";
        let error = registrar.recipe_book_type("first:book").unwrap_err();
        assert!(error.contains("duplicate"));
        assert!(error.contains("first:book"));
        assert!(error.contains("first and second"));
        assert_eq!(registrar.recipe_book_types.len(), 1);
    }

    #[test]
    fn registry_capacity_does_not_evict_existing_books() {
        let mut registrar = PartRegistrar::new("demo");
        for index in 0..1024 {
            registrar
                .recipe_book_type(&format!("demo:book_{index}"))
                .unwrap();
        }
        assert!(
            registrar
                .recipe_book_type("demo:overflow")
                .unwrap_err()
                .contains("1024")
        );
        assert_eq!(registrar.recipe_book_types.len(), 1024);
    }
}
