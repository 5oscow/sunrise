use super::*;
use mod_api::{BlockEntityPart, BlockPart, CanPlaceItemEvent, ItemStack, ModBlockEntity};

struct Filter;

#[mod_api::sunrise_part]
impl BlockPart for Filter {}

#[mod_api::sunrise_part]
impl BlockEntityPart for Filter {
    fn can_place_item(&self, event: &mut CanPlaceItemEvent<'_>) -> Option<bool> {
        assert_eq!(event.entity.id(), "wire_test:filter");
        assert_eq!(event.entity.data, [1]);
        assert_eq!(event.stack.item(), REGISTERED_ITEM.get().unwrap());
        match event.slot {
            0 | 1 => Some(event.slot == event.entity.data[0] as usize && event.stack.count() == 3),
            _ => None,
        }
    }
}

#[test]
fn generated_block_entity_admission_preserves_inputs_and_optional_answer() {
    mod_api::__private::initialize().unwrap();
    install_counter();
    let _phase = mod_api::__private::PhaseOwner::scope(mod_api::__private::ModPhase::World);
    let id = guest::add_handler(vec![guest::view::<dyn BlockEntityPart>(Arc::new(Filter))]);
    let handler = WasmHandler {
        index: id,
        module: WasmMod {
            subscribed: &[],
            listeners: &[],
            group: 0,
            corrupt: false,
        },
    };
    let mut entity = ModBlockEntity::new("wire_test:filter".into(), Default::default());
    entity.data = vec![1];
    let stack = ItemStack::new(REGISTERED_ITEM.get().unwrap(), 3).unwrap();
    for (slot, expected) in [(0, Some(false)), (1, Some(true)), (2, None)] {
        assert_eq!(
            handler.can_place_item(&mut CanPlaceItemEvent {
                entity: &entity,
                slot,
                stack: &stack,
            }),
            expected,
        );
    }
}
