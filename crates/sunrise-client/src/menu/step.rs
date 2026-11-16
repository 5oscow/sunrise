//! Menu operations that can return control at a take without retaining an environment borrow.
use super::{Env, ItemStack, Menu};

/// The positive portion removed from a slot, independent of a result slot's remainder.
#[derive(Clone, Debug)]
pub struct TakeEvent {
    /// Index in the menu's slot table; the slot identifies its backing container and index.
    pub slot: usize,
    /// The items transferred, not the stack left in the source slot.
    pub taken: ItemStack,
    /// The transferred count for a quick move, zero for other takes.
    pub quick_craft_amount: u32,
}

type Continuation<T> = Box<dyn FnOnce(&mut Menu, &Env<'_>) -> MenuStep<T>>;

/// One part of a menu operation. A take returns control before its caller continues.
///
/// Continuations own their local values, not references to the menu or its environment.
/// The coordinator can therefore publish the intermediate menu, run an authoritative
/// effect and construct a fresh environment before resuming. Dropping an unfinished step
/// never executes the rest of the operation.
pub enum MenuStep<T> {
    /// The operation finished.
    Done(T),
    /// The coordinator must handle this take before resuming the operation.
    Take {
        event: TakeEvent,
        resume: Continuation<T>,
    },
}

impl<T: 'static> MenuStep<T> {
    /// Continue with `next` when this operation has finished. Completed steps run inline;
    /// only a suspended operation allocates a continuation.
    pub fn then<U: 'static>(
        self,
        menu: &mut Menu,
        env: &Env<'_>,
        next: impl FnOnce(&mut Menu, &Env<'_>, T) -> MenuStep<U> + 'static,
    ) -> MenuStep<U> {
        match self {
            Self::Done(value) => next(menu, env, value),
            Self::Take { event, resume } => MenuStep::Take {
                event,
                resume: Box::new(move |menu, env| resume(menu, env).then(menu, env, next)),
            },
        }
    }

    /// Run a data-only operation with no external take effects. This is the synchronous
    /// menu API: it retains the original caller's environment for the whole operation.
    pub fn finish(mut self, menu: &mut Menu, env: &Env<'_>) -> T {
        loop {
            match self {
                Self::Done(value) => return value,
                Self::Take { resume, .. } => self = resume(menu, env),
            }
        }
    }
}
