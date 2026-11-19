//! What a pack's `core/clouds` shader reads, against 26.3's `CloudRenderer`: the expected faces
//! are worked out by hand from `buildMesh`, `buildExtrudedCell` and `encodeFace` (the first two
//! cases), or by an independent transcription of them in Python written for these tests (the
//! wrapping case), never by `pack::faces` itself.
use super::pack::{faces, faces_key, info};
use super::*;

/// The input over a `texture`, the camera at block x 0, z -3.96 (cell 0, 0 at time 0) and
/// `above` the clouds' bottom.
fn input(texture: &(u32, u32, Vec<bool>), above: f64, range: i32, fast: bool) -> Input<'_> {
    Input {
        texture,
        uv: [0.0; 2],
        camera: glam::DVec3::new(0.0, CLOUD_HEIGHT + above, -3.96),
        time: 0,
        partial_tick: 0.0,
        range,
        fast,
        color: [1.0, 1.0, 1.0, 0.8],
    }
}

/// One filled cell east of the camera's, the camera inside the layer: up and down (inside, so
/// both), west (its west neighbour empty, x > 0), then its six inward faces, x 1 halved to 0
/// with bit 7 set. `CloudInfo`: the colour through 8 bits (0.8 is 204 of 255), the layer's
/// bottom 2 below the camera, cells 12 by 4 by 12.
#[test]
fn fancy_faces_follow_vanillas_build_mesh() {
    let mut filled = vec![false; 16];
    filled[1] = true;
    let texture = (4, 4, filled);
    let input = input(&texture, 2.0, 1, false);
    assert_eq!(
        faces(&input),
        [
            0, 0, -127, 0, 0, -128, 0, 0, -124, // up, down, west
            0, 0, -112, 0, 0, -111, 0, 0, -110, 0, 0, -109, 0, 0, -108, 0, 0, -107, // inward
        ]
    );
    let info = info(&input);
    let f = |at: usize| f32::from_le_bytes(info[at..at + 4].try_into().unwrap());
    assert_eq!([f(0), f(4), f(8), f(12)], [1.0, 1.0, 1.0, 204.0 / 255.0]);
    assert_eq!([f(16), f(20), f(24)], [0.0, -2.0, 0.0]);
    assert_eq!([f(32), f(36), f(40)], [12.0, 4.0, 12.0]);
}

/// The same cell with the camera above the layer (its bottom 5 below the camera, so its top 1
/// below): no down face, the rest as inside it.
#[test]
fn above_the_layer_a_cell_has_no_down_face() {
    let mut filled = vec![false; 16];
    filled[1] = true;
    let texture = (4, 4, filled);
    assert_eq!(
        faces(&input(&texture, 5.0, 1, false)),
        [
            0, 0, -127, 0, 0, -124, // up, west
            0, 0, -112, 0, 0, -111, 0, 0, -110, 0, 0, -109, 0, 0, -108, 0, 0, -107, // inward
        ]
    );
    // Level with the layer's top (`relativeTopY` 0 is not above it): the down face is there.
    assert_eq!(faces(&input(&texture, 4.0, 1, false))[3..6], [0, 0, -128]);
}

/// Fast clouds: one face a cell, down with the top colour (32). The cell west of the camera's,
/// x -1: halved to -1, bit 7 set, so its third byte is 32 | 128, -96 as a signed byte.
#[test]
fn flat_faces_are_one_down_face_with_the_top_colour() {
    let mut filled = vec![false; 16];
    filled[3] = true;
    let texture = (4, 4, filled);
    assert_eq!(faces(&input(&texture, 2.0, 1, true)), [-1, 0, -96]);
}

/// A 4 by 2 texture with cells 1 and 2 of its first row filled, the camera in cell 0 below the
/// layer, range 3 (4 cells): every face in buildMesh's ring order. Vanilla's neighbours wrap
/// east and west by the texture's height: cell 1's east neighbour is cell 0 (empty), so the
/// copy of cell 1 three cells west gets an east face (-2, 0, -123) that the texture's width
/// would not give it.
#[test]
fn faces_wrap_the_texture_as_vanilla_does() {
    let mut filled = vec![false; 8];
    filled[1] = true;
    filled[2] = true;
    let texture = (4, 2, filled);
    #[rustfmt::skip]
    let expected = [
        0, 0, -128, 0, 0, -124, 0, 0, -112, 0, 0, -111, 0, 0, -110, 0, 0, -109, 0, 0, -108,
        0, 0, -107, -1, 0, 0, 1, 0, 0, -2, 0, -128, -2, 0, -123, 0, -1, -128, 0, -1, -125, 0, -1,
        -124, 0, 1, -128, 0, 1, -126, 0, 1, -124, -1, -1, 0, -1, -1, 3, -1, 1, 0, -1, 1, 2, 1, -1,
        0, 1, -1, 3, 1, 1, 0, 1, 1, 2, -2, -1, -128, -2, -1, -125, -2, -1, -123, -2, 1, -128, -2,
        1, -126, -2, 1, -123,
    ];
    assert_eq!(faces(&input(&texture, -5.0, 3, false)), expected);
}

/// A tall texture (2 by 4), cells 1, 0 and 0, 1 and 0, 2 filled: cell 1, 0's east neighbour
/// wraps by the height to column 2, and cell 0, 2's west one to column 3, both past the image.
/// Vanilla's `getPixel` throws there (the texture never loads); here they count as empty, so
/// each cell gets that face (1, 0's east face is -1, 0, -123), and nothing reads past the row:
/// read as the next row's cell, 1, 0's east neighbour would be 0, 1, filled, and take the face
/// away. Faces from the Python transcription with that one rule.
#[test]
fn a_tall_textures_neighbours_past_the_image_count_as_empty() {
    let mut filled = vec![false; 8];
    filled[1] = true;
    filled[2] = true;
    filled[4] = true;
    let texture = (2, 4, filled);
    #[rustfmt::skip]
    let expected = [
        -1, 0, -128, -1, 0, -123, -1, 0, -112, -1, 0, -111, -1, 0, -110, -1, 0, -109, -1, 0,
        -108, -1, 0, -107, 0, 0, 64, 0, 0, 66, 0, 0, 80, 0, 0, 81, 0, 0, 82, 0, 0, 83, 0, 0, 84,
        0, 0, 85, 0, 0, -128, 0, 0, -124, 0, 0, -112, 0, 0, -111, 0, 0, -110, 0, 0, -109, 0, 0,
        -108, 0, 0, -107, 0, -1, 0, 0, -1, 3, 0, 1, 0,
    ];
    assert_eq!(faces(&input(&texture, -5.0, 1, false)), expected);
}

/// Faces kept while `faces_key` holds are the ones built afresh: along a path through cells and
/// in and out of the layer, an unchanged key never hides changed faces, and the path does
/// change cells and sides.
#[test]
fn the_faces_key_holds_only_while_the_faces_do() {
    let texture = (8, 8, (0..64).map(|i| i % 3 != 0).collect());
    let mut kept: Option<(pack::FacesKey, Vec<i32>)> = None;
    let (mut rebuilt, mut kept_frames) = (0, 0);
    for step in 0..400i32 {
        let t = f64::from(step);
        let input = Input {
            texture: &texture,
            uv: [0.5; 2],
            camera: glam::DVec3::new(t * 0.37, CLOUD_HEIGHT - 6.0 + (t * 0.05) % 14.0, t * 0.37),
            time: i64::from(step),
            partial_tick: 0.0,
            range: 3,
            fast: false,
            color: [1.0; 4],
        };
        let key = faces_key(&input);
        let faces = faces(&input);
        match &kept {
            Some((k, f)) if *k == key => {
                assert_eq!(*f, faces, "step {step}: the key held but the faces changed");
                kept_frames += 1;
            }
            _ => rebuilt += 1,
        }
        kept = Some((key, faces));
    }
    assert!(
        kept_frames > 300 && rebuilt > 10,
        "{kept_frames} kept, {rebuilt} rebuilt"
    );
}
