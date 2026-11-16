//! `ClientRenderClientHost::upload_dynamic_texture` and `release_dynamic_texture` from a wasm
//! mod reach the client with the id, the size and the pixels intact, and each named refusal
//! comes back as the native call returns it (#1343).
use super::*;
use mod_api::prelude::*;
use mod_api::{DynamicTextureError, Identifier};
use std::cell::RefCell;

/// A client that refuses as the game would for these ids and sizes, and records what reached
/// it.
#[derive(Default)]
struct Client {
    uploads: RefCell<Vec<(String, u32, u32, Vec<u8>)>>,
    releases: RefCell<Vec<String>>,
}
impl mod_api::Host for Client {}
mod_api::impl_host_areas!(Client);
impl mod_api::ClientHost for Client {}
mod_api::impl_client_host_areas!(Client, except(client_render));
impl ClientRenderClientHost for Client {
    fn upload_dynamic_texture(
        &self,
        id: &Identifier,
        width: u32,
        height: u32,
        rgba: &[u8],
    ) -> Result<(), DynamicTextureError> {
        self.uploads
            .borrow_mut()
            .push((id.to_string(), width, height, rgba.to_vec()));
        match (id.namespace(), width) {
            ("other", _) => Err(DynamicTextureError::NotOwned),
            (_, 0) => Err(DynamicTextureError::BadSize),
            (_, 4096) => Err(DynamicTextureError::ModBudget),
            (_, 2048) => Err(DynamicTextureError::TotalBudget),
            (_, 1024) => Err(DynamicTextureError::Unavailable),
            _ => Ok(()),
        }
    }
    fn release_dynamic_texture(&self, id: &Identifier) -> Result<(), DynamicTextureError> {
        self.releases.borrow_mut().push(id.to_string());
        match id.path() {
            "panel" => Ok(()),
            _ => Err(DynamicTextureError::Unknown),
        }
    }
}

#[test]
fn a_wasm_mods_texture_reaches_the_client_and_gets_each_refusal_by_name() {
    mod_api::__private::initialize().unwrap();
    let _phase = mod_api::__private::PhaseOwner::scope(mod_api::__private::ModPhase::World);
    let mut client = Client::default();
    let mut frame = boundary::Frame::default();
    frame.set_client_host_mut(&mut client);
    let mut cx = boundary::Cx::default();
    cx.frames.push(frame);
    let cx = RefCell::new(cx);
    guest::set_transport(Some(Rc::new(move |id, bytes| {
        let mut out = Writer::new();
        // SAFETY: client and its frame remain live for this synchronous transport.
        unsafe { boundary::serve(id, &mut *cx.borrow_mut(), bytes, &mut out) }.unwrap();
        out.into_bytes()
    })));
    let id = |text: &str| Identifier::parse(text).unwrap();
    let pixels: Vec<u8> = (0..2 * 3 * 4).collect();
    assert_eq!(
        guest::Guest.upload_dynamic_texture(&id("mymod:panel"), 2, 3, &pixels),
        Ok(())
    );
    let refused = [
        ("other:panel", 2, DynamicTextureError::NotOwned),
        ("mymod:panel", 0, DynamicTextureError::BadSize),
        ("mymod:panel", 4096, DynamicTextureError::ModBudget),
        ("mymod:panel", 2048, DynamicTextureError::TotalBudget),
        ("mymod:panel", 1024, DynamicTextureError::Unavailable),
    ];
    for (texture, width, refusal) in refused {
        let answer = guest::Guest.upload_dynamic_texture(&id(texture), width, 1, &[1, 2, 3, 4]);
        assert_eq!(answer, Err(refusal), "{texture}, {width} wide");
    }
    assert_eq!(
        guest::Guest.release_dynamic_texture(&id("mymod:panel")),
        Ok(())
    );
    assert_eq!(
        guest::Guest.release_dynamic_texture(&id("mymod:gone")),
        Err(DynamicTextureError::Unknown)
    );
    guest::set_transport(None);
    let mut reached = vec![("mymod:panel".to_string(), 2, 3, pixels)];
    for (texture, width, _) in refused {
        reached.push((texture.into(), width, 1, vec![1, 2, 3, 4]));
    }
    assert_eq!(*client.uploads.borrow(), reached);
    assert_eq!(*client.releases.borrow(), ["mymod:panel", "mymod:gone"]);
}
