//! Non-raw loot table completion, distinct from nested raw evaluation.
use super::*;

impl<R: RandomSource + Clone> Context<'_, R> {
    /// Every non-raw entry point completes here with its original context and
    /// random still available. Nested entries call `table_raw` instead: their
    /// functions run before the root splitter and they never complete a query.
    pub(super) fn roll_root(&mut self, id: Option<&str>, table: &Value) -> Vec<LootStack> {
        let mut items = Vec::new();
        self.table_raw(id.unwrap_or("<root>"), table, &mut |context, stack| {
            context.data.split(stack, &mut items)
        });
        // Global modifiers belong after the splitter, with this same context
        // and RNG, before either is returned to the caller (or container fill).
        self.query.finish(id);
        items
    }
}
