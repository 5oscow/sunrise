//! Narrow, per-victim context prepared before the hit borrows its victim.
use super::{Game, combat::PlayerCredit};
use std::collections::HashMap;
use sunrise_client::{
    health::{Attack, Hurting, KnockbackCredit},
    items::ItemStack,
    mobs::{Mob, Species, Who},
};

#[derive(Default)]
pub(super) struct CreditEntity {
    id: i32,
    main: Option<ItemStack>,
}

pub(super) struct CreditPlan {
    victim: i32,
    player: Option<(PlayerCredit, CreditEntity)>,
    previous_mob: CreditEntity,
    attacking_mob: CreditEntity,
    responsibility: Option<Option<(PlayerCredit, CreditEntity)>>,
    fixed: CreditEntity,
}

pub(super) struct CreditLoan<'a> {
    plan: CreditPlan,
    players: &'a mut HashMap<u32, PlayerCredit>,
}

impl CreditPlan {
    pub(super) fn lend(self, players: &mut HashMap<u32, PlayerCredit>) -> CreditLoan<'_> {
        CreditLoan {
            plan: self,
            players,
        }
    }
}

impl KnockbackCredit for CreditLoan<'_> {
    fn accepted_mob_hit(&mut self, mob: &mut Mob) {
        assert_eq!(mob.id as i32, self.plan.victim);
        if let Some(responsibility) = self.plan.responsibility.take() {
            mob.ai.last_hurt_by_player = if responsibility.is_some() { 100 } else { 0 };
            match &responsibility {
                Some((credit, _)) => {
                    self.players.insert(mob.id, *credit);
                }
                None => {
                    self.players.remove(&mob.id);
                }
            }
            self.plan.player = responsibility;
        }
    }

    fn snapshot(&self, pushed: &Hurting<'_>) -> (i32, Option<&ItemStack>) {
        let selected = match pushed {
            Hurting::Mob(mob, _) => {
                assert_eq!(mob.id as i32, self.plan.victim);
                if self.players.contains_key(&mob.id) {
                    // An unresolved remembered player still takes precedence over a mob.
                    self.plan.player.as_ref().map(|(_, entity)| entity)
                } else {
                    match mob.recent_hurt_by() {
                        Some(Who::Mob(id)) if id as i32 == self.plan.attacking_mob.id => {
                            Some(&self.plan.attacking_mob)
                        }
                        Some(Who::Mob(id)) if id as i32 == self.plan.previous_mob.id => {
                            Some(&self.plan.previous_mob)
                        }
                        _ => None,
                    }
                }
            }
            _ => Some(&self.plan.fixed),
        };
        selected.map_or((0, None), |entity| (entity.id, entity.main.as_ref()))
    }
}

impl Game<'_> {
    fn knockback_credit_entity(&self, id: i32) -> CreditEntity {
        if id == 0 || self.entity(id as u32).is_none() {
            return CreditEntity::default();
        }
        let [main, _] = self.attack_of(id).held;
        CreditEntity { id, main }
    }

    fn knockback_player_credit(&self, credit: PlayerCredit) -> (PlayerCredit, CreditEntity) {
        let id = self
            .resolve_player_credit(credit)
            .and_then(|connection| self.player_entity_id_by_connection(connection))
            .unwrap_or(0);
        (credit, self.knockback_credit_entity(id))
    }

    /// Capture only this victim's possible credit holders, not a table of every entity.
    /// UUID responsibility is committed by the loan at the accepted-hit memory point.
    pub(super) fn prepare_knockback_credit(
        &self,
        victim: i32,
        attacker: Option<Who>,
        attack: &Attack,
    ) -> CreditPlan {
        let observed = mod_api::__private::listened(mod_api::HookId::LivingKnockback);
        let mob = observed
            .then(|| self.mobs.iter().find(|mob| mob.id as i32 == victim))
            .flatten();
        let player = mob
            .and_then(|mob| self.last_hurt_by_player.get(&mob.id))
            .copied();
        let responsibility = match attacker {
            Some(Who::Player) => attack.player_uuid.map(|uuid| {
                Some(PlayerCredit(sunrise_client::lan::playerdata::ints_uuid(
                    uuid,
                )))
            }),
            Some(Who::Mob(id)) => self
                .mobs
                .iter()
                .find(|mob| mob.id == id && mob.species() == Species::Wolf && mob.extra.tame)
                .map(|wolf| {
                    wolf.extra
                        .owner
                        .map(|uuid| PlayerCredit(sunrise_client::lan::playerdata::ints_uuid(uuid)))
                }),
            None => None,
        };
        let entity = |id| {
            if observed {
                self.knockback_credit_entity(id)
            } else {
                CreditEntity::default()
            }
        };
        let credited_player = |credit| {
            if observed {
                self.knockback_player_credit(credit)
            } else {
                (credit, CreditEntity::default())
            }
        };
        let previous_mob = mob
            .and_then(|mob| mob.ai.last_hurt_by)
            .and_then(|who| match who {
                Who::Mob(id) => Some(id),
                Who::Player => None,
            });
        let fixed = if observed && mob.is_none() {
            self.entity(victim as u32)
                .and_then(|entity| self.living(entity))
                .and_then(|living| self.living_kill_credit(living))
                .map(|credit| entity(credit.id() as i32))
                .unwrap_or_default()
        } else {
            CreditEntity::default()
        };
        CreditPlan {
            victim,
            player: player.map(credited_player),
            previous_mob: previous_mob.map(|id| entity(id as i32)).unwrap_or_default(),
            attacking_mob: match attacker {
                Some(Who::Mob(id)) => entity(id as i32),
                _ => CreditEntity::default(),
            },
            responsibility: responsibility.map(|credit| credit.map(credited_player)),
            fixed,
        }
    }
}
