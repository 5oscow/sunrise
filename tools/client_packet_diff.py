#!/usr/bin/env python3
"""Compares the serverbound packets of two clients' runs (tools/client_packet_parity.py) route by route, and
keeps only the differences every run reproduces.

  client_packet_diff.py .local/packet-parity/runs                    # vanilla-N against sunrise-N
  client_packet_diff.py RUNS --a vanilla --b sunrise --json report.json

The input is real time, so a key can land a tick earlier in one run than in another: nothing here compares
tick i with tick i. Each route (from its /wire-mark, tools/wire_stream.py) gives, per run:
- shapes: the packet names of each client tick, in order (player_input, move_player_pos, client_tick_end...);
- actions: the sequence of packets that aren't moves, with their fields (block sequence numbers as the step
  from the previous one, entity and container ids as wire_stream names them);
- motion: the per-tick vertical and horizontal deltas of the moves, the yaw and pitch steps, and where the
  route ends (position, yaw, pitch);
- counts: how many of each packet the route sent.
A value is a finding when every run of one client has it and no run of the other does (sets), or when the two
clients' ranges over their runs don't overlap (numbers). Keep-alive answers are checked over each whole run:
every keep_alive the server sends must come back with the same id.
"""
import argparse
from collections import Counter
import difflib
import json
import math
from pathlib import Path

try:
    from . import input_recording, wire_stream
except ImportError:
    import input_recording
    import wire_stream

MOVES = wire_stream.MOVES
# Packets the server's timing decides, or that only the network thread sends: never per tick.
UNTIMED = (wire_stream.FRAME | {"chunk_batch_received", "pong", "keep_alive"}) - {"accept_teleportation"}
POSITION_TOLERANCE = 1e-3
# Counts of a packet differ by a few between runs of one client: where a key lands decides a move or two.
COUNT_SLACK = 2
# Ticks after a route's input ends that still belong to it: the harness waits a second before the next reset.
TAIL_TICKS = 16


def load(path):
    return [json.loads(line) for line in Path(path).read_text().splitlines() if line.strip()]


def played(run_dir, count):
    """[(route name, ticks it lasts)]: from its marker to its input's end plus TAIL_TICKS, never the next reset."""
    path = run_dir / "played.json"
    rows = json.loads(path.read_text()) if path.exists() else []
    out = [(p["route"], math.ceil((p["seconds"] - input_recording.MARKER_SENT) * 20) + TAIL_TICKS) for p in rows]
    return out + [(f"route{i + 1}", None) for i in range(len(out), count)]


def value(v):
    return round(v, 6) if isinstance(v, float) else v


def signature(name, fields, last_sequence):
    """One action as comparable text: a sequence number becomes the step from the previous one (0, which a
    release or a drop always sends, stays 0)."""
    fields = dict(fields)
    if fields.get("sequence"):
        seq = fields.pop("sequence")
        fields["seq_step"] = None if last_sequence[0] is None else seq - last_sequence[0]
        last_sequence[0] = seq
    if name == "use_item":
        fields["yaw"], fields["pitch"] = round(fields["yaw"], 2), round(fields["pitch"], 2)
    if name == "use_item_on":
        fields["cursor"] = [round(c, 4) for c in fields["cursor"]]
    text = ", ".join(f"{k}={value(v)}" for k, v in fields.items())
    return f"{name}{{{text}}}" if text else name


def features(ticks):
    """What one route of one run is compared by."""
    shapes, actions, dys, speeds = Counter(), [], Counter(), Counter()
    first_tick = {}
    counts = Counter()
    last_sequence = [None]
    pos = yaw = pitch = None
    flags = Counter()
    for i, tick in enumerate(ticks):
        names = tuple(n for n, _ in tick["packets"])
        if names:
            shapes[names] += 1
            first_tick.setdefault(("shapes", names), i)
        for name, fields in tick["packets"]:
            counts[name] += 1
            if name in MOVES:
                if "x" in fields:
                    new = (fields["x"], fields["y"], fields["z"])
                    if pos is not None:
                        dy = round(new[1] - pos[1], 5)
                        if dy:
                            dys[dy] += 1
                            first_tick.setdefault(("dy", dy), i)
                        h = round(math.hypot(new[0] - pos[0], new[2] - pos[2]), 4)
                        if h:
                            speeds[h] += 1
                            first_tick.setdefault(("speed", h), i)
                    pos = new
                if "yaw" in fields:
                    yaw, pitch = fields["yaw"], fields["pitch"]
                flags[(fields["on_ground"], fields["horizontal_collision"])] += 1
            elif name not in ("player_input", "client_tick_end"):
                sig = signature(name, fields, last_sequence)
                actions.append(sig)
                first_tick.setdefault(("action", sig), i)
            elif name == "player_input":
                sig = "player_input" + str(fields["keys"])
                actions.append(sig)
                first_tick.setdefault(("action", sig), i)
        for name, _ in tick["frame"]:
            counts[name] += 1
        for name in tick["misplaced"]:
            first_tick.setdefault(("misplaced", name), i)
    # A speed seen once is a transition whose value depends on the tick a key landed on; steady ones repeat.
    speeds = Counter({h: n for h, n in speeds.items() if n >= 2})
    return {"shapes": shapes, "actions": actions, "dy": dys, "speed": speeds, "counts": counts,
            "end": {"pos": pos, "yaw": yaw, "pitch": pitch}, "flags": flags, "first": first_tick,
            "ticks": len(ticks), "misplaced": Counter(n for t in ticks for n in t["misplaced"])}


def keep_alive_problems(entries):
    """Keep-alives the client never answered, or answered with another id."""
    sent, answered = [], set()
    for e in entries:
        if e.get("name") == "keep_alive" and e.get("state") == "play":
            (sent.append if e["dir"] == "s2c" else answered.add)(e.get("data"))
    return [k for k in sent[:-1] if k not in answered]


def runs_of(folder, client):
    out = []
    for run in sorted(folder.glob(f"{client}-*"), key=lambda p: int(p.name.rsplit("-", 1)[1])):
        capture = run / "capture.jsonl"
        if not capture.exists() or not (run / "played.json").exists():
            continue
        entries = load(capture)
        scenarios = wire_stream.scenarios(entries)
        cut = {name: ticks[:length] for (name, length), ticks in zip(played(run, len(scenarios)), scenarios)}
        out.append({"run": run.name, "routes": {n: features(t) for n, t in cut.items()}, "ticks": cut,
                    "keep_alive": keep_alive_problems(entries)})
    return out


def everywhere(runs, route, kind, key):
    return all(key in r["routes"][route][kind] for r in runs)


def nowhere(runs, route, kind, key):
    return not any(key in r["routes"][route][kind] for r in runs)


def example(runs, route, kind, key):
    r = runs[0]
    return {"run": r["run"], "tick": r["routes"][route]["first"].get((kind, key))}


def excerpt(run, route, tick, around=1):
    ticks = run["ticks"][route]
    if tick is None:
        return []
    lines = []
    for i in range(max(0, tick - around), min(len(ticks), tick + around + 1)):
        parts = []
        for name, fields in ticks[i]["packets"]:
            text = ", ".join(f"{k}={value(v)}" for k, v in fields.items())
            parts.append(f"{name}{{{text}}}" if text else name)
        lines.append(f"tick {i}: " + "; ".join(parts))
    return lines


def compare_sets(a, b, route, kind, names):
    found = []
    keys = set().union(*(r["routes"][route][kind] for r in a + b))
    for key in sorted(keys, key=str):
        for this, other, side in ((a, b, "only_" + names[0]), (b, a, "only_" + names[1])):
            if everywhere(this, route, kind, key) and nowhere(other, route, kind, key):
                ex = example(this, route, kind, key)
                found.append({"route": route, "kind": kind, "side": side, "value": list(key)
                              if isinstance(key, tuple) else key, "example": ex,
                              "excerpt": excerpt(this[0], route, ex["tick"])})
    return found


def disjoint(xs, ys, tolerance=0.0):
    return max(xs) + tolerance < min(ys) or max(ys) + tolerance < min(xs)


def compare_numbers(a, b, route, names):
    found = []
    fa, fb = [r["routes"][route] for r in a], [r["routes"][route] for r in b]
    for field in ("yaw", "pitch"):
        xs, ys = [f["end"][field] for f in fa], [f["end"][field] for f in fb]
        if None not in xs + ys and disjoint(xs, ys, 0.01):
            found.append({"route": route, "kind": "end_" + field, names[0]: xs, names[1]: ys})
    xs, ys = [f["end"]["pos"] for f in fa], [f["end"]["pos"] for f in fb]
    if None not in xs + ys:
        for axis, label in enumerate("xyz"):
            if disjoint([p[axis] for p in xs], [p[axis] for p in ys], POSITION_TOLERANCE):
                found.append({"route": route, "kind": "end_" + label, names[0]: [p[axis] for p in xs],
                              names[1]: [p[axis] for p in ys]})
    for name in sorted(set().union(*(f["counts"] for f in fa + fb)) - UNTIMED - {"chat_command"}):
        xs, ys = [f["counts"][name] for f in fa], [f["counts"][name] for f in fb]
        if disjoint(xs, ys, COUNT_SLACK):
            found.append({"route": route, "kind": "count", "packet": name, names[0]: xs, names[1]: ys})
    for name in sorted(set().union(*(f["misplaced"] for f in fb)) - set().union(*(f["misplaced"] for f in fa))):
        if all(f["misplaced"][name] for f in fb):
            found.append({"route": route, "kind": "misplaced", "packet": name, names[1]: [f["misplaced"][name] for f in fb]})
    return found


def compare_actions(a, b, route, names):
    """Differences of the action sequences that every pair of runs (one of each client) shows."""
    seen = None
    for ra in a:
        for rb in b:
            sa, sb = ra["routes"][route]["actions"], rb["routes"][route]["actions"]
            chunks = set()
            for op, i1, i2, j1, j2 in difflib.SequenceMatcher(None, sa, sb, autojunk=False).get_opcodes():
                if op != "equal":
                    chunks.add((tuple(sa[i1:i2]), tuple(sb[j1:j2])))
            seen = chunks if seen is None else seen & chunks
    return [{"route": route, "kind": "actions", names[0]: list(x), names[1]: list(y)} for x, y in sorted(seen or [])]


def report(a, b, names=("vanilla", "sunrise")):
    routes = [r for r in a[0]["routes"] if all(r in run["routes"] for run in a + b)]
    findings = []
    for route in routes:
        for kind in ("shapes", "dy", "speed"):
            findings += compare_sets(a, b, route, kind, names)
        findings += compare_actions(a, b, route, names)
        findings += compare_numbers(a, b, route, names)
    for runs, name in ((a, names[0]), (b, names[1])):
        if all(r["keep_alive"] for r in runs):
            findings.append({"route": "*", "kind": "keep_alive_unanswered", "client": name,
                             "runs": {r["run"]: len(r["keep_alive"]) for r in runs}})
    return {"runs": {names[0]: [r["run"] for r in a], names[1]: [r["run"] for r in b]}, "routes": routes,
            "findings": findings}


def report_runs(folder, names=("vanilla", "sunrise")):
    a, b = runs_of(Path(folder), names[0]), runs_of(Path(folder), names[1])
    if not a or not b:
        raise SystemExit(f"{folder}: needs finished runs of both {names[0]} and {names[1]}")
    return report(a, b, names)


def describe(rep):
    lines = [f"runs: {rep['runs']}", f"routes: {len(rep['routes'])}", f"findings: {len(rep['findings'])}"]
    for f in rep["findings"]:
        rest = {k: v for k, v in f.items() if k not in ("route", "kind", "excerpt")}
        lines.append(f"- [{f['route']}] {f['kind']}: {json.dumps(rest)[:400]}")
        for line in f.get("excerpt", [])[:3]:
            lines.append(f"      {line[:300]}")
    return "\n".join(lines)


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("runs", type=Path)
    parser.add_argument("--a", default="vanilla")
    parser.add_argument("--b", default="sunrise")
    parser.add_argument("--json", type=Path)
    args = parser.parse_args()
    rep = report_runs(args.runs, (args.a, args.b))
    if args.json:
        args.json.write_text(json.dumps(rep, indent=1))
    print(describe(rep))


if __name__ == "__main__":
    main()
