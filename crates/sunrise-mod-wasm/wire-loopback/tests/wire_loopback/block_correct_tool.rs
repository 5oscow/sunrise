//! `LevelRef::block_correct_tool` from a wasm mod reaches the backend with the level, the
//! position and the tool intact, and the answer comes back as the native read's.
use super::*;
use mod_api::__private::game::{BackendMutRefOf, BackendRefOf};
use mod_api::{ItemStack, LevelRef, WorldEpoch};

thread_local! {
    static ASKED: std::cell::RefCell<Vec<([i32; 3], Option<u32>)>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// A tool of one item is correct at x 1, no tool is; nothing is at x 2; x 3 is not loaded.
fn answer(pos: [i32; 3], tool: Option<&ItemStack>) -> Option<bool> {
    match pos[0] {
        1 => Some(tool.is_some_and(|t| t.count() == 1)),
        2 => Some(false),
        _ => None,
    }
}

struct Server;
impl mod_api::Host for Server {
    fn backend(&self) -> Option<mod_api::BackendRef<'_>> {
        Some(mod_api::BackendRef::of(self))
    }
}
mod_api::impl_host_areas!(Server);
impl mod_api::ServerHost for Server {
    fn backend_mut(&mut self) -> Option<mod_api::BackendMutRef<'_>> {
        Some(mod_api::BackendMutRef::of(self))
    }
}
mod_api::impl_server_host_areas!(Server);
impl mod_api::Backend for Server {}
mod_api::impl_backend_areas!(Server, except(levels));
impl mod_api::prelude::LevelsBackend for Server {
    fn level_block_correct_tool(
        &self,
        _level: LevelRef,
        pos: [i32; 3],
        tool: Option<&ItemStack>,
    ) -> Option<bool> {
        ASKED.with_borrow_mut(|asked| asked.push((pos, tool.map(ItemStack::count))));
        answer(pos, tool)
    }
}
impl mod_api::BackendMut for Server {}
mod_api::impl_backend_mut_areas!(Server);

fn asks() -> Vec<([i32; 3], Option<ItemStack>)> {
    let item = REGISTERED_ITEM.get().unwrap();
    let one = ItemStack::new(item, 1).unwrap();
    let two = ItemStack::new(item, 2).unwrap();
    vec![
        ([1, 64, 0], Some(one.clone())),
        ([1, 64, 0], Some(two)),
        ([1, 64, 0], None),
        ([2, 64, 0], Some(one.clone())),
        ([3, 64, 0], Some(one)),
    ]
}

#[test]
fn a_wasm_mods_correct_tool_read_crosses_whole() {
    ASKED.with_borrow_mut(Vec::clear);
    // The fixture mod registers the item the tools are stacks of.
    install_counter();
    let _phase = mod_api::__private::PhaseOwner::scope(mod_api::__private::ModPhase::World);
    let level = LevelRef::new(WorldEpoch::new(2), mod_api::DimensionKey::OVERWORLD);
    let mut server = Server;
    {
        let host = &mut server;
        let mut frame = boundary::Frame::default();
        frame.set_server_host_mut(host);
        let mut cx = Box::new(boundary::Cx::default());
        cx.frames.push(frame);
        let pointer = &mut *cx as *mut boundary::Cx;
        // SAFETY: the context and borrowed host outlive synchronous proxy calls.
        let transport = unsafe { host_dispatch::transport(pointer) };
        let previous =
            LOOPBACK_TRANSPORT.with(|active| active.borrow_mut().replace(transport.clone()));
        let _restore = TransportScope(previous);
        guest::set_transport(Some(transport));
        for (pos, tool) in asks() {
            assert_eq!(
                level.block_correct_tool(&guest::Guest, pos, tool.as_ref()),
                answer(pos, tool.as_ref())
            );
        }
        guest::set_transport(None);
    }
    let expected: Vec<_> = asks()
        .into_iter()
        .map(|(pos, tool)| (pos, tool.map(|t| t.count())))
        .collect();
    assert_eq!(ASKED.with_borrow_mut(std::mem::take), expected);
}
