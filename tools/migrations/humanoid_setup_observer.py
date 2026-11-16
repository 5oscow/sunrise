#!/usr/bin/env python3
"""Thread a frame-local animation observer through the existing render-data path."""
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
BASE = ROOT / "crates/sunrise-client/src/mobs"


def main():
    for name in ("render.rs", "invisible_body.rs"):
        path = BASE / name
        text = path.read_text()
        main, separator, tests = text.partition("#[cfg(test)]")
        main = main.replace("data: &MobData", "data: &dyn RenderDataSource")
        import re
        main = re.sub(r"\bdata\s*\.\s*(animations|cem|random_textures|blinks|emissive)\b", r"data.mob_data().\1", main)
        main = main.replace("anim::state(m, alpha)", "anim::state(m, alpha).observed(m, data.humanoid_setup())")
        if name == "invisible_body.rs":
            main = main.replace("Atlas, Mob, MobData, Part", "Atlas, Mob, RenderDataSource, Part")
        path.write_text(main + separator + tests)


if __name__ == "__main__":
    main()
