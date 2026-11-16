//! Concrete authoring extracted verbatim from the vocabulary layer.
use super::*;

/// Fixtures that tests of other crates use to install Parts.
#[cfg(any(test, feature = "testing"))]
pub mod tests {
    #[cfg(test)]
    use super::*;
    #[cfg(test)]
    use crate::*;

    /// A feature type twice, one in an unowned namespace, one without a name and one taking a
    /// vanilla feature type's id: every registration is refused at startup, naming the mod.
    #[test]
    pub(crate) fn duplicate_foreign_and_vanilla_feature_types_refuse_registration() {
        struct Tree;
        impl mod_api::FeatureType for Tree {
            fn place(
                &self,
                _: &mut dyn mod_api::FeatureContext,
                _: [i32; 3],
            ) -> Result<bool, mod_api::FeatureError> {
                Ok(false)
            }
        }
        struct Valid;
        #[mod_api::__subscriptions]
        impl Hooks for Valid {
            fn register(&self, r: &mut dyn Registrar, _: &mod_api::RegistrationContext) {
                r.register_feature_type("tree", Tree);
            }
        }
        crate::parts::tests::install_parts_fixture(&[ModEntry::new("test", Box::new(Valid))])
            .unwrap();
        let types = crate::__private::feature_type_declarations();
        assert_eq!(
            types.iter().map(|(id, _)| id.as_str()).collect::<Vec<_>>(),
            ["test:tree"]
        );

        struct Invalid;
        #[mod_api::__subscriptions]
        impl Hooks for Invalid {
            fn register(&self, r: &mut dyn Registrar, _: &mod_api::RegistrationContext) {
                r.register_feature_type("tree", Tree);
                r.register_feature_type("tree", Tree);
                r.register_feature_type("other:tree", Tree);
                r.register_feature_type("", Tree);
                r.register_feature_type("minecraft:tree", Tree);
            }
        }
        let errors = install_parts(
            &[ModEntry::new("test", Box::new(Invalid))],
            &crate::parts::tests::rule_schema(),
            |_| false,
        )
        .unwrap_err();
        assert_eq!(errors.len(), 4, "{errors:?}");
        assert!(errors[0].contains("test registered feature type test:tree twice"));
        assert!(errors[1].contains("namespace"));
        assert!(errors[2].contains("invalid identifier \"test:\""));
        assert!(errors[3].contains("namespace"), "{}", errors[3]);
    }
}

impl FeaturesRegistrar for PartRegistrar {
    fn add_feature_type(&mut self, name: &str, ty: Arc<dyn crate::FeatureType>) {
        let id = match self.owned_declaration_id(name) {
            Ok(id) => id,
            Err(reason) => {
                self.refused.push(reason);
                return;
            }
        };
        let refusal = {
            self.feature_types
                .iter()
                .any(|t| *t.id == *id)
                .then(|| format!("{} registered feature type {id} twice", self.mod_id))
        };
        if let Some(refusal) = refusal {
            self.refused.push(refusal);
            return;
        }
        self.feature_types
            .push(FeatureTypeInfo { id: id.into(), ty });
    }
}
