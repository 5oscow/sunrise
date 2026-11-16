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
        // Every non-raw roll completes after the splitter with its own
        // context and random, before the caller consumes the list.
        self.query.finish(id);
        items
    }
}
