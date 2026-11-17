use super::*;
pub(crate) fn resolved_effects() -> &'static OnceLock<sunrise_mod_runtime::registry_keys::Effects> {
    static EMPTY: OnceLock<sunrise_mod_runtime::registry_keys::Effects> = OnceLock::new();
    installed().map_or(&EMPTY, |installed| &installed.effects)
}

/// Registrar operations for effects.
pub trait EffectsRegistrar {
    /// Request an effect by canonical registry id. All mods' declarations are checked at load.
    fn effect_key(&mut self, id: &str) -> crate::EffectKey;
    /// Attach tick behaviour. The declaring package or a declared dependent may attach.
    /// Missing optional dependencies skip their Parts; other unknown effects refuse loading.
    fn attach_mob_effect_part(
        &mut self,
        effect: &crate::EffectKey,
        kind: &'static str,
        part: Arc<dyn MobEffectPart>,
    ) -> Result<(), MobEffectPartError>;
}

/// A refused effect Part registration.
#[derive(Clone, Debug, PartialEq, Eq, sunrise_mod_api_macros::Wire)]
#[non_exhaustive]
pub enum MobEffectPartError {
    /// Vanilla effects keep vanilla tick behaviour.
    VanillaEffect {
        /// The vanilla effect that was requested.
        effect: crate::EffectKey,
    },
    /// This concrete Part type is already attached to the effect.
    Duplicate {
        /// The effect already carrying this Part type.
        effect: crate::EffectKey,
        /// The concrete Rust Part type name.
        part: String,
    },
}

impl dyn crate::Registrar + '_ {
    /// Attach a Part to an effect declared by this package or a dependency.
    /// Parts execute in dependency dispatch order, then registration order;
    /// each Part's cadence applies only to that Part. Vanilla types refuse immediately.
    pub fn mob_effect_part<P: MobEffectPart>(
        &mut self,
        effect: &crate::EffectKey,
        part: P,
    ) -> Result<(), MobEffectPartError> {
        self.attach_mob_effect_part(effect, type_name::<P>(), Arc::new(part))
    }
}

/// Validate Part permissions against the actual package that declared each effect.
/// Call after effect-key completion, so unknown effects retain their ordinary refusal.
pub fn validate_effect_part_owners<'a>(
    owner: impl Fn(&str) -> Option<&'a str>,
) -> Result<(), Vec<String>> {
    let Some(installed) = installed() else {
        return Ok(());
    };
    let mut ids: Vec<_> = installed.keys.effects.keys().collect();
    ids.sort_unstable();
    let mut refused = Vec::new();
    for id in ids {
        let Some(owner) = owner(id) else {
            for part in &installed.keys.effects[id] {
                refused.push(format!("mod {} requested unknown effect {id}", part.mod_id));
            }
            continue;
        };
        for part in &installed.keys.effects[id] {
            if part.mod_id == owner {
                continue;
            }
            let allowed = installed
                .effect_part_packages
                .iter()
                .find(|package| package.id() == part.mod_id)
                .is_some_and(|package| {
                    package.dependencies().iter().any(|dependency| {
                        dependency.id() == owner
                            && matches!(
                                dependency.kind(),
                                crate::DependencyKind::Required | crate::DependencyKind::Optional
                            )
                    })
                });
            if !allowed {
                refused.push(format!("mod {} attached {} to effect {id}, declared by package {owner}; declare {owner} as a dependency", part.mod_id, part.type_name));
            }
        }
    }
    if refused.is_empty() {
        Ok(())
    } else {
        Err(refused)
    }
}

impl HookParts {
    pub(crate) fn effect<'k>(
        &self,
        id: impl FnOnce() -> Option<&'k str>,
    ) -> &[Attached<dyn MobEffectPart>] {
        attached(&self.effects, id)
    }
}

/// Run each effect Part's cadence and action as one pair, sharing the holder snapshots.
/// Queued writes remain deferred until this entire dispatch returns.
pub fn dispatch_mob_effect_tick(
    host: &dyn Host,
    tick_count: i32,
    call: crate::__private::ApplyEffectTickCall<'_>,
) {
    if !has_part_hook(HookId::ShouldApplyEffectTickThisTick) {
        return;
    }
    let Some(id) = crate::content::registry_keys::effect_id(call.effect) else {
        return;
    };
    let table = parts(HookId::ShouldApplyEffectTickThisTick, "effect tick routing");
    let Some(attached) = table.effects.get(id) else {
        return;
    };
    for part in crate::entered_parts(attached, "mob_effect_tick") {
        let mut should = crate::ShouldApplyEffectTickThisTickEvent {
            effect: call.effect,
            tick_count,
            amplifier: call.amplifier,
            apply: false,
        };
        part.should_apply_effect_tick_this_tick(host, &mut should);
        if !should.apply {
            continue;
        }
        let mut event = crate::ApplyEffectTickEvent {
            effect: call.effect,
            dimension: call.dimension,
            entity: call.entity,
            amplifier: call.amplifier,
            holder: call.holder,
            health: call.health,
            max_health: call.max_health,
            food: call.food,
            active_effects: call.active_effects,
            changes: &mut *call.changes,
            keep: *call.keep,
        };
        part.apply_effect_tick(host, &mut event);
        *call.keep = event.keep;
        if !*call.keep {
            break;
        }
    }
}
