#!/usr/bin/env python3
"""Compare how far block entities are drawn with the vanilla 26.3 client renderers.

The Java oracle (VanillaBlockEntityReachOracle) lists every block whose block entity has a
renderer, then asks that renderer BlockEntityRenderDispatcher's questions for each case:
shouldRenderOffScreen, getViewDistance and shouldRender (camera position and render distance).
`examples/probe/block_entity_reach_fixture` answers the same cases with Sunrise's `Reach`, which
every block entity draw asks (the BlockEntityRenderer path, beacon and test instance beams,
structure and test instance outlines, moving pistons, campfire, brushable, shelf, vault and
spawner items). Cameras sit on each view distance's edge, one float step either side of it,
and half a block inside, around block positions near the origin, below zero and near the
world border. Every value must match exactly.

  test_block_entity_reach_parity.py [--filter SUBSTRING] [--detail N]
"""
import argparse
import json
import math
import os
from pathlib import Path
import subprocess
import sys

try:
    from . import lab, test_rust_parity as base, test_model_baking_parity as baking
except ImportError:
    import lab, test_rust_parity as base, test_model_baking_parity as baking

PARITY = lab.ROOT / 'mods/sunrise/src/test/java/dev/sunriselab/parity'
POSITIONS = [[0, 64, 0], [-37, -20, 1205], [29_999_983, 100, -29_999_983], [5, 319, -8]]
# The fixed view distances (getViewDistance), seen at a short and a long render distance,
# and the beacon's, 16 blocks per chunk of render distance, at its own.
FIXED = [64, 68, 96, 256]
RENDER_DISTANCES = [2, 6, 12, 32]
EDGES = [(r, d) for r in FIXED for d in (2, 32)] + [(16 * d, d) for d in RENDER_DISTANCES]


def cameras(pos):
    """(camera, render distance) around every edge: on it, one float step either side, half a block inside."""
    centre = [c + 0.5 for c in pos]
    out = []
    for r, render_distance in EDGES:
        for d in (r, math.nextafter(r, 0), math.nextafter(r, math.inf), r - 0.5, r + 0.5):
            s = d / math.sqrt(2)
            for camera in (
                [centre[0] + d, centre[1], centre[2]],
                [centre[0], centre[1], centre[2] - d],
                [centre[0] + s, centre[1], centre[2] - s],
                # Straight above: BeaconRenderer measures across the ground only.
                [centre[0], centre[1] + d, centre[2]],
                [centre[0] - s, centre[1] + 37.25, centre[2] + s],
                # From the block's corner, not its centre.
                [pos[0] - d, pos[1], pos[2]],
            ):
                out.append((camera, render_distance))
    return out


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--filter', default='')
    parser.add_argument('--detail', type=int, default=3)
    args = parser.parse_args()
    lock, jars, _ = base.verify_artifacts()
    libraries = baking.client_libraries()
    build = lab.LOCAL / 'rust-block-entity-reach-parity'
    build.mkdir(parents=True, exist_ok=True)
    classes = build / 'classes'
    classes.mkdir(exist_ok=True)
    dependencies = os.pathsep.join(map(str, [jars['client'], *libraries]))

    def run(command, name, cwd=lab.ROOT):
        with (build / (name + '.log')).open('w') as log:
            result = subprocess.run(list(map(str, command)), cwd=cwd, timeout=3600, stdout=log, stderr=subprocess.STDOUT)
        if result.returncode:
            print((build / (name + '.log')).read_text()[-4000:])
            sys.exit(f'{name} failed')

    run(['javac', '--release', lock['minecraft']['java_major'], '-proc:none', '-nowarn', '-cp', dependencies, '-d', classes,
         *(PARITY / f'{n}.java' for n in ('ParityData', 'ParityFont', 'VanillaTextDisplayRenderOracle',
                                          'VanillaBlockEntityReachOracle'))], 'compile-java')
    java = ['java', '-Xmx2G', '-Djava.awt.headless=true', '-cp', str(classes) + os.pathsep + dependencies,
            'dev.sunriselab.parity.VanillaBlockEntityReachOracle']
    run([*java, 'list', build / 'blocks.txt'], 'vanilla-list')
    blocks = [b for b in (build / 'blocks.txt').read_text().split() if args.filter in b]
    assert blocks, 'no block entity renderers listed'
    chosen = [{'block': block, 'pos': pos, 'camera': camera, 'renderDistance': d}
              for block in blocks for pos in POSITIONS for camera, d in cameras(pos)]
    inputs = build / 'inputs.jsonl'
    inputs.write_text(''.join(json.dumps(c) + '\n' for c in chosen))
    run([*java, 'cases', inputs, build / 'vanilla.jsonl'], 'vanilla')
    run(['cargo', 'run', '--quiet', '--profile', 'fast', '-p', 'sunrise-client-lib', '--example', 'client_probe', '--',
         'block_entity_reach_fixture', inputs, build / 'rust.jsonl'], 'rust')
    vanilla = [json.loads(l) for l in (build / 'vanilla.jsonl').read_text().splitlines()]
    rust = [json.loads(l) for l in (build / 'rust.jsonl').read_text().splitlines()]
    assert len(vanilla) == len(rust) == len(chosen), (len(vanilla), len(rust), len(chosen))
    problems, errors, failed = {}, {}, 0
    for case, v, r in zip(chosen, vanilla, rust):
        if 'error' in v:
            errors.setdefault(case['block'], v['error'])
            failed += 1
            continue
        differ = [k for k in ('offScreen', 'viewDistance', 'shouldRender') if v[k] != r[k]]
        if differ:
            problems.setdefault(case['block'], []).append((case, differ))
    for block, error in sorted(errors.items()):
        print(f'ORACLE ERROR {block}: {error[:300]}')
    for block, items in sorted(problems.items()):
        fields = sorted({k for _, differ in items for k in differ})
        print(f'MISMATCH {block}: {len(items)} cases ({", ".join(fields)})')
        for case, differ in items[:args.detail]:
            print('   ', json.dumps(case), differ)
    failed += sum(len(i) for i in problems.values())
    print(f'{len(chosen) - failed} of {len(chosen)} cases match over {len(blocks)} blocks; '
          f'oracle errors on {len(errors)} blocks')
    if problems or errors:
        sys.exit(1)
    print('block entity reach parity ok')


if __name__ == '__main__':
    main()
