use super::*;

impl Game<'_> {
    /// The mods' state mesh recipes (`section_mesh_recipes`), asked on the world thread once per
    /// resource load and on `refresh_render_visibility`. Client-side.
    pub(in crate::game) fn native_section_mesh_recipes(
        &self,
        call: mod_api::__private::SectionMeshRecipesCall<'_>,
    ) {
        self.fire_section_mesh_recipes(call);
    }
}
