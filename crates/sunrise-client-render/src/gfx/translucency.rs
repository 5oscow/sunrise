//! Vanilla 26.3 MeshData.SortState / VertexSorting.byDistance.
//! Keep vertex order intact; only the quad indices change as the camera moves.

/// MeshData.decodeQuadCentroids uses the opposite corners, not all four vertices.
pub fn quad_center(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|i| (a[i] + b[i]) / 2.0)
}

/// The side an axis-aligned quad faces: the terrain pipelines cull back faces, so a quad whose
/// plane the camera is behind draws no pixel. Water's surface has a back face beside its front
/// (`FluidState.shouldRenderBackwardUpFace`), so about half of a lake's quads are such.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Facing {
    axis: usize,
    /// The plane's coordinate on `axis`, section-relative.
    plane: f32,
    /// Whether the front (the counter-clockwise side) looks towards `+axis`.
    positive: bool,
}

/// How far a camera's coordinate can be behind a plane without the quad being culled by float
/// error in the projection.
const PLANE_MARGIN: f32 = 1.0 / 256.0;

impl Facing {
    /// The facing of the quad `corners` (in draw order, as `quad_center`'s), decided as the
    /// rasterizer decides it, per triangle by winding: a quad's triangles are 0 1 2 and 2 3 0,
    /// and each one's front is the side its counter-clockwise winding looks from. Only a quad
    /// whose four corners share one coordinate on an axis and whose two triangles wind the same
    /// way has one facing; any other (a sloped surface, a lopsided or bow-tie quad) is `None` and
    /// always drawn, as one of its triangles may face a camera the other does not.
    pub fn of(corners: &[[f32; 3]; 4]) -> Option<Self> {
        let axis = (0..3).find(|&a| corners.iter().all(|c| c[a] == corners[0][a]))?;
        let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
        // The sign of a triangle's winding along `axis`: its edges' cross product there.
        let winding = |a: usize, b: usize, c: usize| {
            let (e1, e2) = (
                [corners[b][u] - corners[a][u], corners[b][v] - corners[a][v]],
                [corners[c][u] - corners[a][u], corners[c][v] - corners[a][v]],
            );
            e1[0] * e2[1] - e1[1] * e2[0]
        };
        let (first, second) = (winding(0, 1, 2), winding(2, 3, 0));
        (first != 0.0 && (first > 0.0) == (second > 0.0) && second != 0.0).then_some(Self {
            axis,
            plane: corners[0][axis],
            positive: first > 0.0,
        })
    }

    /// Whether a camera anywhere in `reach` (its lowest and highest coordinate on each axis) is
    /// behind the quad, or in its plane: it draws nothing from there.
    fn hidden_from(self, reach: &[[f32; 2]; 3]) -> bool {
        let [low, high] = reach[self.axis];
        if self.positive {
            high <= self.plane - PLANE_MARGIN
        } else {
            low >= self.plane + PLANE_MARGIN
        }
    }
}

#[derive(Default)]
pub struct Sorter {
    order: Vec<(u32, f32)>,
    indices: Vec<u32>,
}

impl Sorter {
    pub fn sort(&mut self, centers: &[[f32; 3]], camera: [f32; 3]) -> &[u32] {
        self.order.clear();
        self.order.extend(centers.iter().enumerate().map(|(i, c)| {
            let [x, y, z] = std::array::from_fn(|j| camera[j] - c[j]);
            (i as u32, x * x + y * y + z * z)
        }));
        // VertexSorting uses a stable merge sort, preserving mesh order for ties.
        self.order.sort_by(|a, b| b.1.total_cmp(&a.1));
        self.indices.clear();
        for &(quad, _) in &self.order {
            let v = quad * 4;
            self.indices.extend([v, v + 1, v + 2, v + 2, v + 3, v]);
        }
        &self.indices
    }
}

impl Sorter {
    /// `sort`, then the quads some camera in `reach` draws, as it left them, before the ones
    /// none does (`Facing::hidden_from`), in their order: the quads' count first.
    fn sort_facing(
        &mut self,
        centers: &[[f32; 3]],
        facings: &[Option<Facing>],
        camera: [f32; 3],
        reach: &[[f32; 2]; 3],
    ) -> (&[u32], usize) {
        self.sort(centers, camera);
        let hidden = |quad: u32| facings[quad as usize].is_some_and(|f| f.hidden_from(reach));
        // `sort_by` is stable: both runs keep the back to front order.
        self.order.sort_by_key(|&(quad, _)| hidden(quad));
        let kept = self.order.partition_point(|&(quad, _)| !hidden(quad));
        self.indices.clear();
        for &(quad, _) in &self.order {
            let v = quad * 4;
            self.indices.extend([v, v + 1, v + 2, v + 2, v + 3, v]);
        }
        (&self.indices, kept)
    }
}

/// Which side of a section's 16 block cube the camera is on, in each axis: -1 below or before
/// it, 1 above or after it, 0 within its extent (`TranslucencyPointOfView`).
pub fn viewpoint(camera: [f64; 3], origin: [i32; 3]) -> [i32; 3] {
    std::array::from_fn(|i| {
        let block = camera[i].floor() as i32;
        ((block >> 4) - (origin[i] >> 4)).clamp(-1, 1)
    })
}

pub struct SortState {
    centers: Vec<[f32; 3]>,
    facings: Vec<Option<Facing>>,
    /// When the last order left quads out (`update`'s `cull_back`): the viewpoint it was made for
    /// (`viewpoint`) and how many quads at its front a camera there draws.
    kept: Option<([i32; 3], usize)>,
    last: Option<([i32; 3], [i32; 3])>,
}

impl SortState {
    pub fn new(centers: Vec<[f32; 3]>) -> Self {
        Self {
            facings: vec![None; centers.len()],
            centers,
            kept: None,
            last: None,
        }
    }

    /// The state of a mesh's translucent quads, each as its four corners.
    pub fn of_quads(quads: impl Iterator<Item = [[f32; 3]; 4]>) -> Self {
        let (centers, facings) = quads
            .map(|q| (quad_center(q[0], q[2]), Facing::of(&q)))
            .unzip();
        Self {
            centers,
            facings,
            kept: None,
            last: None,
        }
    }

    /// How many of the section's `total` quads the camera's draw takes, which are the first
    /// quads of its indices: all unless the last order left some out for the camera's own
    /// viewpoint `pov` (`viewpoint`). A section is re-sorted only now and then, so a camera that
    /// has moved to another viewpoint since draws the whole order until it is.
    pub fn drawn(&self, total: u32, cull_back: bool, pov: [i32; 3]) -> u32 {
        match self.kept {
            Some((made_for, kept)) if cull_back && made_for == pov => kept as u32,
            _ => total,
        }
    }

    pub fn is_sorted(&self) -> bool {
        self.last.is_some()
    }

    /// Large tiles cannot use the section viewpoint shortcut: the camera may
    /// cross a quad's plane while remaining on the same side of the first 16³.
    pub fn update_lod<'a>(
        &mut self,
        camera: [f64; 3],
        origin: [i32; 3],
        sorter: &'a mut Sorter,
    ) -> Option<&'a [u32]> {
        let block = camera.map(|v| v.floor() as i32);
        if self.last.is_some_and(|(last, _)| last == block) {
            return None;
        }
        self.last = Some((block, [0; 3]));
        self.kept = None;
        Some(sorter.sort(
            &self.centers,
            std::array::from_fn(|i| (camera[i] - f64::from(origin[i])) as f32),
        ))
    }

    /// TranslucencyPointOfView and ResortTransparencyTask: diagonal views keep
    /// their ordering until the point of view changes; aligned views also
    /// resort on crossing a block boundary. Sub-block movement needs no upload.
    ///
    /// With `cull_back` the order has the quads that face the camera first (`drawn`), and a quad
    /// is left out only when no camera of this viewpoint could see its front: a quad's plane is
    /// on one axis, and the camera must be off the section on that axis, on the quad's back
    /// side.
    pub fn update<'a>(
        &mut self,
        camera: [f64; 3],
        origin: [i32; 3],
        cull_back: bool,
        sorter: &'a mut Sorter,
    ) -> Option<&'a [u32]> {
        let block = camera.map(|v| v.floor() as i32);
        let pov = viewpoint(camera, origin);
        if let Some((last_block, last_pov)) = self.last
            && last_pov == pov
            && (last_block == block || !pov.contains(&0))
            && self.kept.is_some() == cull_back
        {
            return None;
        }
        self.last = Some((block, pov));
        let relative = std::array::from_fn(|i| (camera[i] - f64::from(origin[i])) as f32);
        if !cull_back {
            self.kept = None;
            return Some(sorter.sort(&self.centers, relative));
        }
        // Every camera of the viewpoint, section-relative: beyond the section's edge on an axis
        // it is off, and on an axis it shares with the section, anywhere in it.
        let reach = pov.map(|side| match side {
            1.. => [16.0, f32::INFINITY],
            0 => [0.0, 16.0],
            _ => [f32::NEG_INFINITY, 0.0],
        });
        let (indices, kept) = sorter.sort_facing(&self.centers, &self.facings, relative, &reach);
        self.kept = Some((pov, kept));
        Some(indices)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equal_distances_keep_mesh_order_and_triangle_winding() {
        let mut sorter = Sorter::default();
        assert_eq!(
            sorter.sort(
                &[[1.0, 0.0, 0.0], [-1.0, 0.0, 0.0], [4.0, 0.0, 0.0]],
                [0.0; 3]
            ),
            &[8, 9, 10, 10, 11, 8, 0, 1, 2, 2, 3, 0, 4, 5, 6, 6, 7, 4]
        );
        assert!(sorter.sort(&[], [0.0; 3]).is_empty());
    }

    #[test]
    fn movement_reverses_faces_without_moving_vertices() {
        let centers = [[0.5, 0.5, 0.0], [0.5, 0.5, 1.0]];
        let mut sorter = Sorter::default();
        assert_eq!(sorter.sort(&centers, [0.5, 0.5, -2.0])[0], 4);
        assert_eq!(sorter.sort(&centers, [0.5, 0.5, 3.0])[0], 0);
    }

    #[test]
    fn cached_viewpoint_handles_negative_and_large_coordinates() {
        let origin = [-30_000_000, -64, 30_000_000];
        let mut state = SortState::new(vec![[0.5; 3]]);
        let mut sorter = Sorter::default();
        let camera = [-29_999_999.5, -63.5, 30_000_000.5];
        assert!(state.update(camera, origin, false, &mut sorter).is_some());
        assert!(state.update(camera, origin, false, &mut sorter).is_none());
        assert!(
            state
                .update(
                    [-29_999_999.1, -63.1, 30_000_000.9],
                    origin,
                    false,
                    &mut sorter
                )
                .is_none()
        );
        assert!(
            state
                .update(
                    [-29_999_998.9, -63.1, 30_000_000.9],
                    origin,
                    false,
                    &mut sorter
                )
                .is_some()
        );
    }
    #[test]
    fn large_tiles_resort_inside_the_same_section_viewpoint() {
        let mut state = SortState::new(vec![[32.0, 0.0, 0.0], [96.0, 0.0, 0.0]]);
        let mut sorter = Sorter::default();
        assert_eq!(
            state
                .update_lod([48.0, 20.0, 20.0], [0; 3], &mut sorter)
                .unwrap()[0],
            4
        );
        assert!(
            state
                .update_lod([48.5, 20.0, 20.0], [0; 3], &mut sorter)
                .is_none()
        );
        // Both views have positive x/y/z signs relative to the first section,
        // so the section-sized shortcut would incorrectly keep the old order.
        assert_eq!(
            state
                .update_lod([80.0, 20.0, 20.0], [0; 3], &mut sorter)
                .unwrap()[0],
            0
        );
    }

    /// A water block's surface: the front quad looking up, the back one looking down.
    fn surface(y: f32, front: bool) -> [[f32; 3]; 4] {
        let corners = [[0.0, y, 0.0], [0.0, y, 1.0], [1.0, y, 1.0], [1.0, y, 0.0]];
        if front {
            corners
        } else {
            [corners[0], corners[3], corners[2], corners[1]]
        }
    }

    fn pov(camera: [f64; 3], origin: [i32; 3]) -> [i32; 3] {
        viewpoint(camera, origin)
    }

    #[test]
    fn a_surface_draws_its_front_to_a_camera_above_and_its_back_to_one_below() {
        let quads = [surface(8.0, true), surface(8.0, false)];
        let mut sorter = Sorter::default();
        let mut state = SortState::of_quads(quads.into_iter());
        // In another section, above: only quad 0 (indices 0..4) is drawn.
        let above = [8.5, 40.5, 8.5];
        let indices = state.update(above, [0, 0, 0], true, &mut sorter).unwrap();
        assert_eq!(indices[0], 0);
        let up = pov(above, [0; 3]);
        assert_eq!(
            (state.drawn(2, true, up), state.drawn(2, false, up)),
            (1, 2)
        );
        // Below it: only quad 1 (indices 4..8).
        let below = [8.5, -20.5, 8.5];
        let indices = state.update(below, [0, 0, 0], true, &mut sorter).unwrap();
        assert_eq!(indices[0], 4);
        assert_eq!(state.drawn(2, true, pov(below, [0; 3])), 1);
    }

    #[test]
    fn a_camera_in_the_sections_own_extent_sees_every_quad() {
        let mut sorter = Sorter::default();
        let mut state = SortState::of_quads([surface(8.5, true), surface(8.5, false)].into_iter());
        // At any height within the section's 16 blocks, above the plane or below.
        for y in [0.2, 8.4, 8.6, 15.9] {
            let camera = [8.5, y, 8.5];
            state.update(camera, [0; 3], true, &mut sorter);
            assert_eq!(state.drawn(2, true, pov(camera, [0; 3])), 2, "at y {y}");
        }
    }

    #[test]
    fn a_camera_that_moved_to_another_viewpoint_draws_the_whole_order_until_it_is_sorted() {
        let mut sorter = Sorter::default();
        let mut state = SortState::of_quads([surface(8.0, true), surface(8.0, false)].into_iter());
        // Sorted from above: the back quad is out.
        let above = [8.5, 40.5, 8.5];
        state.update(above, [0; 3], true, &mut sorter);
        assert_eq!(state.drawn(2, true, pov(above, [0; 3])), 1);
        // The camera dived below the section before the section was sorted again: the quad the
        // sort left out now faces it, and it draws.
        let below = [8.5, -20.5, 8.5];
        assert_eq!(state.drawn(2, true, pov(below, [0; 3])), 2);
        // Back above, nothing was re-sorted: the prefix is good again.
        assert_eq!(state.drawn(2, true, pov(above, [0; 3])), 1);
    }

    fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
        [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
    }

    /// Whether the rasterizer draws triangle `a b c` for a camera at `eye`: its winding, as the
    /// geometric normal's side of the eye, counter-clockwise from the front.
    fn triangle_faces(a: [f32; 3], b: [f32; 3], c: [f32; 3], eye: [f32; 3]) -> bool {
        let (u, v, w) = (sub(b, a), sub(c, a), sub(eye, a));
        let normal = [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ];
        normal[0] * w[0] + normal[1] * w[1] + normal[2] * w[2] > 0.0
    }

    /// Whether either of a quad's triangles (0 1 2 and 2 3 0) faces `eye`.
    fn quad_faces(q: &[[f32; 3]; 4], eye: [f32; 3]) -> bool {
        triangle_faces(q[0], q[1], q[2], eye) || triangle_faces(q[2], q[3], q[0], eye)
    }

    /// The quads kept draw in exactly the order the whole list had them: back to front by
    /// distance, ties in mesh order.
    #[test]
    fn the_kept_quads_keep_the_full_back_to_front_order() {
        let mut quads = Vec::new();
        for n in 0..40 {
            let y = 1.0 + (n % 7) as f32 * 0.5;
            let (x, z) = ((n % 5) as f32, (n / 5 % 4) as f32);
            let top = [
                [x, y, z],
                [x, y, z + 1.0],
                [x + 1.0, y, z + 1.0],
                [x + 1.0, y, z],
            ];
            quads.push(if n % 2 == 0 {
                top
            } else {
                [top[0], top[3], top[2], top[1]]
            });
        }
        let centers: Vec<[f32; 3]> = quads.iter().map(|q| quad_center(q[0], q[2])).collect();
        let camera = [-3.5, 40.5, 2.5];
        let full = Sorter::default().sort(&centers, [-3.5, 40.5, 2.5]).to_vec();
        let mut sorter = Sorter::default();
        let mut state = SortState::of_quads(quads.iter().copied());
        let order = state
            .update(camera, [0; 3], true, &mut sorter)
            .unwrap()
            .to_vec();
        let kept = state.drawn(quads.len() as u32, true, pov(camera, [0; 3])) as usize;
        assert!(kept > 0 && kept < quads.len());
        let in_prefix: Vec<u32> = order[..kept * 6].to_vec();
        let expected: Vec<u32> = full
            .chunks(6)
            .filter(|quad| in_prefix.chunks(6).any(|kept| kept == *quad))
            .flatten()
            .copied()
            .collect();
        assert_eq!(in_prefix, expected);
    }

    #[test]
    fn a_quad_whose_triangles_wind_opposite_ways_is_never_left_out() {
        // A bow-tie in one plane: triangle 0 1 2 winds one way, 2 3 0 the other.
        let bow = [
            [0.0, 3.0, 0.0],
            [1.0, 3.0, 1.0],
            [1.0, 3.0, 0.0],
            [0.0, 3.0, 1.0],
        ];
        assert!(
            triangle_faces(bow[0], bow[1], bow[2], [0.5, 9.0, 0.5])
                != triangle_faces(bow[2], bow[3], bow[0], [0.5, 9.0, 0.5])
        );
        assert_eq!(Facing::of(&bow), None);
        // A zero-area triangle has no winding to decide by.
        let flat = [
            [0.0, 3.0, 0.0],
            [1.0, 3.0, 0.0],
            [2.0, 3.0, 0.0],
            [0.0, 3.0, 1.0],
        ];
        assert_eq!(Facing::of(&flat), None);
    }

    #[test]
    fn a_sloped_quad_stays_while_either_triangle_faces_the_camera() {
        // A flowing surface's top: three corners at 0.5 and one at 0.9, so its triangles lie
        // in different planes (0 1 2 rises towards corner 2, 2 3 0 is level).
        let top = [
            [0.0, 0.5, 0.0],
            [0.0, 0.5, 1.0],
            [1.0, 0.9, 1.0],
            [1.0, 0.5, 0.0],
        ];
        assert_eq!(Facing::of(&top), None);
        // Grazing from the side, a little below the level triangle's plane but above the
        // raised one: only triangle 0 1 2 faces the camera.
        let eye = [-5.0, 0.49, 0.5];
        assert!(triangle_faces(top[0], top[1], top[2], eye));
        assert!(!triangle_faces(top[2], top[3], top[0], eye));
        let mut sorter = Sorter::default();
        let mut state = SortState::of_quads([top].into_iter());
        let camera = [-4.5, 0.49, 0.5];
        state.update(camera, [0; 3], true, &mut sorter);
        assert_eq!(state.drawn(1, true, pov(camera, [0; 3])), 1);
    }

    /// Quads on planes at both sides of the section and at the block edges, both windings, from
    /// cameras all around: a quad left out faces no camera of the viewpoint, which is judged here
    /// by each triangle's winding at sample points of the viewpoint's reach, whatever the quad's
    /// position in the section.
    #[test]
    fn left_out_quads_face_away_from_every_camera_of_the_viewpoint() {
        let mut quads = Vec::new();
        for plane in [0.0, 0.44, 3.5, 7.89, 9.56, 15.99, 16.0] {
            let faces = [
                [
                    [0.0, plane, 0.0],
                    [0.0, plane, 1.0],
                    [1.0, plane, 1.0],
                    [1.0, plane, 0.0],
                ],
                [
                    [plane, 0.0, 0.0],
                    [plane, 1.0, 0.0],
                    [plane, 1.0, 1.0],
                    [plane, 0.0, 1.0],
                ],
                [
                    [0.0, 0.0, plane],
                    [1.0, 0.0, plane],
                    [1.0, 1.0, plane],
                    [0.0, 1.0, plane],
                ],
            ];
            for face in faces {
                quads.push(face);
                // The same quad with the winding reversed, as water's back face is.
                quads.push([face[0], face[3], face[2], face[1]]);
            }
        }
        assert!(quads.iter().all(|q| Facing::of(q).is_some()));
        let mut sorter = Sorter::default();
        let origin = [32, -16, 64];
        let mut left_out = 0;
        for x in [-9.5_f64, 3.2, 20.5, 31.9] {
            for y in [-30.2_f64, -16.0, -5.5, 2.5, 40.0] {
                for z in [-20.5_f64, 70.4, 80.5, 100.0] {
                    let camera = [
                        f64::from(origin[0]) + x,
                        f64::from(origin[1]) + y,
                        f64::from(origin[2]) + z,
                    ];
                    let mut state = SortState::of_quads(quads.iter().copied());
                    let indices = state
                        .update(camera, origin, true, &mut sorter)
                        .unwrap()
                        .to_vec();
                    let side = pov(camera, origin);
                    let kept = state.drawn(quads.len() as u32, true, side) as usize;
                    // Cameras of the viewpoint, section-relative: three samples an axis.
                    let eyes: Vec<[f32; 3]> = (0..27)
                        .map(|n| {
                            [0, 1, 2].map(|i| {
                                let [low, high] = match side[i] {
                                    1.. => [16.0, 40.0],
                                    0 => [0.0, 16.0],
                                    _ => [-24.0, -0.001],
                                };
                                low + (high - low) * ((n / 3_usize.pow(i as u32)) % 3) as f32 / 2.0
                            })
                        })
                        .collect();
                    for quad in indices.chunks(6).skip(kept) {
                        left_out += 1;
                        let q = &quads[(quad[0] / 4) as usize];
                        assert!(
                            eyes.iter().all(|&eye| !quad_faces(q, eye)),
                            "{q:?} is left out of the viewpoint {side:?} but faces a camera"
                        );
                    }
                }
            }
        }
        assert!(left_out > 0);
    }
}
