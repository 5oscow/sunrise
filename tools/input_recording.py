#!/usr/bin/env python3
"""Raw keyboard and mouse recordings, recorded from the real devices and replayed into an X display through XTEST,
so two game clients get exactly the same input (tools/client_packet_parity.py).

  input_recording.py record run.jsonl                    # every keyboard and mouse; F8 marks, F9 stops
  input_recording.py record run.jsonl --device /dev/input/event5 --device /dev/input/event7
  input_recording.py replay run.jsonl --display :42      # into that display's focused window
  input_recording.py show run.jsonl

A recording is JSON lines. The first is the header ({"format": "sunrise-input-recording", "version": 1}); every
other line is one event with `t`, seconds since the first marker:
  {"t": 0.0, "type": "marker", "route": "walk"}       # the replayer sends the chat command /wire-mark here
  {"t": 0.5, "type": "key", "code": "KEY_W", "down": true}
  {"t": 0.6, "type": "button", "code": "BTN_LEFT", "down": true}
  {"t": 0.7, "type": "rel", "dx": 12, "dy": -3}       # raw mouse counts, as the device reported them
  {"t": 0.8, "type": "wheel", "dy": -1}               # one notch; negative is towards the user
Codes are evdev's names (KEY_*, BTN_*). Key repeats are not recorded: the replay display has autorepeat off.

Recording reads /dev/input (the `input` group) without grabbing the devices: the game keeps getting them. It
starts at the first marker key and stops at the stop key; the marker and stop keys themselves are not recorded.
Replay sets the display's pointer acceleration to 1 so a relative move reaches the game as the same counts the
device sent (raw input), as on a desktop with a flat acceleration profile.
"""
import argparse
import json
import sys
import time
from pathlib import Path

FORMAT = {"format": "sunrise-input-recording", "version": 1}
MARKER_COMMAND = "wire-mark"
# evdev names that don't map to an X keysym by their lower-case suffix.
KEYSYMS = {
    "KEY_LEFTCTRL": "Control_L", "KEY_RIGHTCTRL": "Control_R", "KEY_LEFTSHIFT": "Shift_L",
    "KEY_RIGHTSHIFT": "Shift_R", "KEY_LEFTALT": "Alt_L", "KEY_RIGHTALT": "Alt_R", "KEY_SPACE": "space",
    "KEY_ESC": "Escape", "KEY_ENTER": "Return", "KEY_TAB": "Tab", "KEY_BACKSPACE": "BackSpace",
    "KEY_SLASH": "slash", "KEY_MINUS": "minus", "KEY_EQUAL": "equal", "KEY_DOT": "period", "KEY_COMMA": "comma",
    "KEY_SEMICOLON": "semicolon", "KEY_APOSTROPHE": "apostrophe", "KEY_GRAVE": "grave",
    "KEY_LEFTBRACE": "bracketleft", "KEY_RIGHTBRACE": "bracketright", "KEY_BACKSLASH": "backslash",
    "KEY_UP": "Up", "KEY_DOWN": "Down", "KEY_LEFT": "Left", "KEY_RIGHT": "Right", "KEY_CAPSLOCK": "Caps_Lock",
    "KEY_DELETE": "Delete", "KEY_HOME": "Home", "KEY_END": "End", "KEY_PAGEUP": "Prior", "KEY_PAGEDOWN": "Next",
    "KEY_INSERT": "Insert",
}
BUTTONS = {"BTN_LEFT": 1, "BTN_MIDDLE": 2, "BTN_RIGHT": 3, "BTN_SIDE": 8, "BTN_EXTRA": 9}
# Characters the marker command types, as evdev keys (a US layout, Xvfb's default).
CHARS = {"/": "KEY_SLASH", "-": "KEY_MINUS", " ": "KEY_SPACE"}


def keysym_name(code):
    if code in KEYSYMS:
        return KEYSYMS[code]
    name = code.removeprefix("KEY_")
    if len(name) == 1:
        return name.lower()
    if name.startswith("F") and name[1:].isdigit():
        return name
    raise ValueError(f"no X keysym for {code}")


def char_key(c):
    return CHARS.get(c) or f"KEY_{c.upper()}"


def write(path, events):
    with open(path, "w") as out:
        out.write(json.dumps(FORMAT) + "\n")
        for e in sorted(events, key=lambda e: e["t"]):
            out.write(json.dumps(e) + "\n")


def read(path):
    lines = [json.loads(line) for line in Path(path).read_text().splitlines() if line.strip()]
    if not lines or {k: lines[0].get(k) for k in FORMAT} != FORMAT:
        raise ValueError(f"{path}: not a {FORMAT['format']} v{FORMAT['version']} file")
    return lines[1:]


def routes(events):
    """The recording cut at its markers: [(route name, events with t from that marker)]."""
    out = []
    for e in events:
        if e["type"] == "marker":
            out.append((e.get("route", f"route{len(out) + 1}"), [], e["t"]))
        if out:
            out[-1][1].append(dict(e, t=e["t"] - out[-1][2]))
    return [(name, evs) for name, evs, _ in out]


def marker_events(t, command=MARKER_COMMAND):
    """The keys that send `/command` from the chat: T opens it, the text, Enter. 60 ms a key, so a 20 fps client
    still sees each press and release in its own frame."""
    out, step = [], 0.06
    keys = ["KEY_T"] + [char_key(c) for c in "/" + command] + ["KEY_ENTER"]
    for i, key in enumerate(keys):
        start = t + i * 2 * step + (0.3 if i else 0)
        out += [{"t": start, "type": "key", "code": key, "down": True},
                {"t": start + step, "type": "key", "code": key, "down": False}]
    return out


MARKER_SECONDS = 0.3 + (len(MARKER_COMMAND) + 3) * 0.12 + 0.6
# When Enter sends the marker, from the marker event: a route's ticks start there.
MARKER_SENT = 0.3 + (len(MARKER_COMMAND) + 2) * 0.12


class Injector:
    """XTEST input into one X display, to whatever window has its focus."""

    def __init__(self, display_name):
        from Xlib import X, display
        from Xlib.ext import xtest
        self.X, self.xtest = X, xtest
        self.d = display.Display(display_name)
        if not self.d.has_extension("XTEST"):
            raise RuntimeError(f"{display_name} has no XTEST")
        self.d.change_pointer_control(accel=(1, 1), threshold=1)
        self.d.change_keyboard_control(auto_repeat_mode=X.AutoRepeatModeOff)
        self.held = set()
        self.d.sync()

    def keycode(self, code):
        from Xlib import XK
        sym = XK.string_to_keysym(keysym_name(code))
        kc = self.d.keysym_to_keycode(sym)
        if not kc:
            raise ValueError(f"the display has no key for {code}")
        return kc

    def send(self, e):
        X, fake = self.X, self.xtest.fake_input
        kind = e["type"]
        if kind == "key":
            fake(self.d, X.KeyPress if e["down"] else X.KeyRelease, self.keycode(e["code"]))
            (self.held.add if e["down"] else self.held.discard)(("key", e["code"]))
        elif kind == "button":
            fake(self.d, X.ButtonPress if e["down"] else X.ButtonRelease, BUTTONS[e["code"]])
            (self.held.add if e["down"] else self.held.discard)(("button", e["code"]))
        elif kind == "rel":
            fake(self.d, X.MotionNotify, True, x=int(e["dx"]), y=int(e["dy"]))
        elif kind == "wheel":
            button = 4 if e["dy"] > 0 else 5
            for _ in range(abs(int(e["dy"]))):
                fake(self.d, X.ButtonPress, button)
                fake(self.d, X.ButtonRelease, button)
        self.d.flush()

    def release_all(self):
        for kind, code in sorted(self.held):
            self.send({"type": kind, "code": code, "down": False})

    def play(self, events, start=None):
        """Sends each event at its time from `start` (default now); a marker becomes the chat command."""
        expanded = []
        for e in events:
            expanded += marker_events(e["t"]) if e["type"] == "marker" else [e]
        expanded.sort(key=lambda e: e["t"])
        start = time.perf_counter() if start is None else start
        for e in expanded:
            wait = start + e["t"] - time.perf_counter()
            if wait > 0.002:
                time.sleep(wait - 0.001)
            while time.perf_counter() < start + e["t"]:
                pass
            self.send(e)
        self.release_all()
        return time.perf_counter() - start


def record(path, devices, marker, stop):
    import evdev
    from evdev import ecodes
    from select import select
    if devices:
        found = [evdev.InputDevice(p) for p in devices]
    else:
        found = []
        for p in evdev.list_devices():
            dev = evdev.InputDevice(p)
            caps = dev.capabilities()
            keys, rels = caps.get(ecodes.EV_KEY, []), caps.get(ecodes.EV_REL, [])
            if ecodes.KEY_W in keys or ecodes.REL_X in rels:
                found.append(dev)
    if not found:
        sys.exit("no keyboard or mouse readable in /dev/input (is this user in the `input` group?)")
    for dev in found:
        print(f"reading {dev.path}: {dev.name}", file=sys.stderr)
    print(f"press {marker} to start a route, {stop} to stop", file=sys.stderr)
    events, t0, rel, routes_seen = [], None, {}, 0
    by_fd = {dev.fd: dev for dev in found}
    while True:
        ready, _, _ = select(by_fd, [], [])
        for fd in ready:
            for ev in by_fd[fd].read():
                now = ev.timestamp()
                if ev.type == ecodes.EV_KEY and ev.value in (0, 1):
                    code = ecodes.KEY.get(ev.code) or ecodes.BTN.get(ev.code)
                    code = code[0] if isinstance(code, list) else code
                    if code == stop and ev.value == 1:
                        write(path, events)
                        print(f"{len(events)} events, {routes_seen} routes: {path}", file=sys.stderr)
                        return
                    if code == marker:
                        if ev.value == 1:
                            t0 = now if t0 is None else t0
                            routes_seen += 1
                            events.append({"t": round(now - t0, 6), "type": "marker",
                                           "route": f"route{routes_seen}"})
                        continue
                    if t0 is None or code is None:
                        continue
                    kind = "button" if code.startswith("BTN_") else "key"
                    events.append({"t": round(now - t0, 6), "type": kind, "code": code, "down": ev.value == 1})
                elif ev.type == ecodes.EV_REL and t0 is not None:
                    if ev.code == ecodes.REL_WHEEL:
                        events.append({"t": round(now - t0, 6), "type": "wheel", "dy": ev.value})
                    elif ev.code in (ecodes.REL_X, ecodes.REL_Y):
                        rel.setdefault(fd, [now, 0, 0])
                        rel[fd][1 if ev.code == ecodes.REL_X else 2] += ev.value
                elif ev.type == ecodes.EV_SYN and fd in rel:
                    when, dx, dy = rel.pop(fd)
                    events.append({"t": round(when - t0, 6), "type": "rel", "dx": dx, "dy": dy})


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="command", required=True)
    r = sub.add_parser("record")
    r.add_argument("out", type=Path)
    r.add_argument("--device", action="append", default=[], help="an evdev device (default: every keyboard and mouse)")
    r.add_argument("--marker", default="KEY_F8")
    r.add_argument("--stop", default="KEY_F9")
    p = sub.add_parser("replay")
    p.add_argument("recording", type=Path)
    p.add_argument("--display", required=True, help="an X display, never the desktop's own")
    p.add_argument("--route", help="only this route")
    s = sub.add_parser("show")
    s.add_argument("recording", type=Path)
    args = parser.parse_args()
    if args.command == "record":
        record(args.out, args.device, args.marker, args.stop)
    elif args.command == "replay":
        injector = Injector(args.display)
        for name, events in routes(read(args.recording)):
            if args.route in (None, name):
                print(f"{name}: {injector.play(events):.1f} s")
    else:
        for name, events in routes(read(args.recording)):
            kinds = {}
            for e in events:
                kinds[e["type"]] = kinds.get(e["type"], 0) + 1
            print(f"{name}: {events[-1]['t']:.1f} s, {kinds}")


if __name__ == "__main__":
    main()
