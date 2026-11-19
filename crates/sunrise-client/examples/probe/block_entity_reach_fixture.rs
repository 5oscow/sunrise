//! Headless transport for the block-entity reach parity gate: for each case (block, block
//! position, camera, render distance) writes what `Reach` decides: off screen, view distance
//! and whether the block entity is drawn. Every block entity draw asks `Reach`.
#![forbid(unsafe_code)]
use glam::DVec3;
use serde_json::{Value, json};
use std::{
    fs,
    io::{BufWriter, Write},
    path::PathBuf,
};
use sunrise_client::block_entity_reach::Reach;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

pub fn main() -> Result<()> {
    let args: Vec<PathBuf> = crate::args_os().skip(1).map(PathBuf::from).collect();
    let [inputs, output] = args.as_slice() else {
        return Err("usage: block_entity_reach_fixture INPUTS OUTPUT".into());
    };
    let mut out = BufWriter::new(fs::File::create(output)?);
    for line in fs::read_to_string(inputs)?.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let case: Value = serde_json::from_str(line)?;
        let reach = Reach::of(case["block"].as_str().ok_or("case without block")?);
        let int = |v: &Value| v.as_i64().map(|n| n as i32).ok_or("not an integer");
        let float = |v: &Value| v.as_f64().ok_or("not a number");
        let pos = [
            int(&case["pos"][0])?,
            int(&case["pos"][1])?,
            int(&case["pos"][2])?,
        ];
        let camera = DVec3::new(
            float(&case["camera"][0])?,
            float(&case["camera"][1])?,
            float(&case["camera"][2])?,
        );
        let render_distance = int(&case["renderDistance"])?;
        writeln!(
            out,
            "{}",
            json!({
                "offScreen": reach.off_screen(),
                "viewDistance": reach.view_distance(render_distance),
                "shouldRender": reach.should_render(pos, camera, render_distance),
            })
        )?;
    }
    out.flush()?;
    Ok(())
}
