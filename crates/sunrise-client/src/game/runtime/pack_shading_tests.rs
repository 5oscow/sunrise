//! A pack folder's Polytone expression uniforms reach the game as its pack resources read them
//! (`PackResources::load`, `pack_shading`): the lightmap's are named for its shaders and evaluate
//! with the frame's expression context; a folder without them gives none.
use sunrise_assets::resource_source::ResourceSource;

#[test]
fn a_pack_folders_shader_modifiers_reach_the_pack_resources() {
    struct Scratch(std::path::PathBuf);
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let scratch = Scratch(std::env::temp_dir().join(format!(
        "sunrise-pack-shading-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    )));
    let modifiers = scratch
        .0
        .join("assets/minecraft/polytone/shader_modifiers/core");
    std::fs::create_dir_all(&modifiers).unwrap();
    let folder = ResourceSource::directory(scratch.0.clone());
    assert!(super::resource_tables::pack_shading(&folder).1.is_none());
    std::fs::write(
        modifiers.join("lightmap.json"),
        r#"{"Tint": "g.dimensionType == 'minecraft:the_nether' ? 1 : 0.5"}"#,
    )
    .unwrap();
    let (post, modifiers) = super::resource_tables::pack_shading(&folder);
    assert!(post.is_none(), "no post chains");
    let modifiers = modifiers.expect("the folder's modifiers");
    let shaders = sunrise_client::gfx::LIGHTMAP_SHADERS;
    assert_eq!(modifiers.names(&shaders), ["Tint"]);
    let mut wanted = [("Tint".to_owned(), 0.0)];
    let frame = sunrise_client::environment::Frame {
        dimension: "minecraft:the_nether".into(),
        ..Default::default()
    };
    modifiers.evaluate(&shaders, &frame, &mut wanted);
    assert_eq!(wanted[0].1, 1.0);
}
