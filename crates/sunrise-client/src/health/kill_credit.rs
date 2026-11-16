//! The hit-local kill-credit loan, separate from the level's block/random loan.
use super::Hurting;
use crate::{items::ItemStack, mobs::Mob};

/// Resolves the pushed entity's credit from its current hurt memories. Implemented by the
/// game's narrow per-hit context; it does not expose the game through a level host.
pub trait KnockbackCredit {
    /// Commit accepted-hit player responsibility before the default push.
    fn accepted_mob_hit(&mut self, mob: &mut Mob);
    /// The credit entity and its main hand. Zero means a genuinely unresolved credit.
    fn snapshot(&self, pushed: &Hurting<'_>) -> (i32, Option<&ItemStack>);
}
