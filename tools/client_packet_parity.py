#!/usr/bin/env python3
"""Client packet parity for anticheats: the vanilla 26.3 client and Sunrise's, each in its own Xvfb display, play
the same input recording on the same vanilla 26.3 server, and their serverbound packets are compared
(tools/client_packet_diff.py). Anticheats (Grim, Vulcan, Polar...) judge a client by exactly these packets.

  client_packet_parity.py world                                 # once: the seeded world with the test course
  client_packet_parity.py run --client vanilla --runs 3         # every route of client_packet_routes.py
  client_packet_parity.py run --client sunrise --runs 3 --bin target/fast
  client_packet_parity.py run --client sunrise --recording mine.jsonl   # a person's own (input_recording.py)
  client_packet_parity.py diff                                  # every run of both: the report
  client_packet_parity.py all --runs 3                          # world, both clients, diff

Every run restores the world snapshot first (course, player data, time and weather), starts the vanilla
dedicated server (offline, from the verified jars in .local, tools/lab.py prepare), puts tools/net_capture.py
between it and the client, and starts the client as Steve on a fresh Xvfb display, never the desktop's. Before
each route the server's console resets the player (place, inventory, effects), the client settles for 40
ticks, and the route's input goes in through XTEST, starting with its marker (the chat command /wire-mark,
where tools/wire_stream.py cuts the stream). Both clients get the same options: window, GUI scale, mouse
sensitivity, raw input, key bindings, frame cap, render distance and minimal graphics (OPTIONS).

The PvP routes (pvp_*) fight a second player, Opp0: sunrise-bots --behaviour stand (build it with
`cargo build --profile fast -p sunrise-bots`), which stands where the console puts it; the knockback the client
takes comes from `damage ... by Opp0` at set times of the route.

Runs land in .local/packet-parity/runs/<client>-<n>/ (capture.jsonl, logs, a screenshot after each route);
--label names them otherwise, and `client_packet_diff.py RUNS --a <label> --b <label>` compares any two sets.
Run it at low priority beside other work: `nice -n 19 ionice -c3 python3 tools/client_packet_parity.py ...`.
"""
import argparse
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import threading
import time

sys.path.insert(0, str(Path(__file__).resolve().parent))
import client_packet_routes as routes  # noqa: E402
import input_recording  # noqa: E402
import lab  # noqa: E402
import net_capture  # noqa: E402
import test_rust_parity as base  # noqa: E402
from test_dedicated_server_parity import Server  # noqa: E402
from test_wire_stream_parity import ClientCounts, settle  # noqa: E402

WORK = lab.LOCAL / "packet-parity"
SNAPSHOT = WORK / "world-snapshot"
PLAYER = routes.PLAYER
SEED = "20261119"
PROPERTIES = {
    "online-mode": "false", "white-list": "false", "level-seed": SEED, "generate-structures": "false",
    "view-distance": "6", "simulation-distance": "6", "spawn-protection": "0", "pause-when-empty-seconds": "-1",
    "gamemode": "survival", "force-gamemode": "true", "difficulty": "easy", "spawn-monsters": "false",
    "enable-command-block": "false", "level-name": "world", "max-tick-time": "-1", "sync-chunk-writes": "false",
}
# One options.txt for both clients: what Sunrise doesn't read it ignores.
OPTIONS = {
    "onboardAccessibility": "false", "skipMultiplayerWarning": "true", "joinedFirstServer": "true",
    "tutorialStep": "none", "pauseOnLostFocus": "false", "renderDistance": "4", "simulationDistance": "5",
    "maxFps": "60", "enableVsync": "false", "inactivityFpsLimit": "minimized", "guiScale": str(routes.GUI_SCALE),
    "mouseSensitivity": "0.5", "rawMouseInput": "true", "autoJump": "false", "toggleCrouch": "false",
    "toggleSprint": "false", "toggleAttack": "false", "toggleUse": "false", "discrete_mouse_scroll": "false",
    "mouseWheelSensitivity": "1.0", "invertYMouse": "false", "narrator": "0", "soundCategory_master": "0.0",
    "realmsNotifications": "false", "chatLinksPrompt": "false", "onlyShowSecureChat": "false",
    "renderClouds": '"false"', "particles": "2", "entityShadows": "false", "mipmapLevels": "0", "ao": "false",
    "biomeBlendRadius": "0", "graphicsPreset": '"fast"', "bobView": "false", "fov": "0.0", "lang": "en_us",
}
GAMERULES = ["advance_time false", "advance_weather false", "spawn_mobs false", "spawn_patrols false",
             "spawn_wandering_traders false", "spawn_phantoms false", "fire_spread_radius_around_player 0",
             "random_tick_speed 0", "respawn_radius 0"]
TERRAIN_CANDIDATES = [(-40, 20), (-40, 60), (-40, 100), (-44, -8), (96, 20), (96, 100)]
LVP = Path("/usr/share/vulkan/icd.d/lvp_icd.json")


def server_command():
    _, jars, libraries = base.verify_artifacts()
    deps = os.pathsep.join(map(str, [jars["server"], *libraries]))
    return ["java", "-Xmx2g", "-cp", deps, "net.minecraft.server.Main", "nogui"]


def server_folder(directory, port):
    directory.mkdir(parents=True, exist_ok=True)
    props = dict(PROPERTIES, **{"server-port": str(port), "server-ip": "127.0.0.1"})
    (directory / "server.properties").write_text("".join(f"{k}={v}\n" for k, v in props.items()),
                                                 encoding="iso-8859-1")
    # The owner accepted Mojang's EULA for these runs (https://aka.ms/MinecraftEULA).
    (directory / "eula.txt").write_text("eula=true\n")


def ask(server, command, pattern, timeout=60):
    """Runs a console command and returns the first new log line matching `pattern`."""
    seen = len(server.lines)
    server.console(command)
    deadline = time.time() + timeout
    regex = re.compile(pattern)
    while time.time() < deadline:
        for line in server.lines[seen:]:
            if regex.search(line):
                return line
        time.sleep(0.1)
    raise RuntimeError(f"no answer to {command!r} matching {pattern!r}:\n" + "\n".join(server.lines[-10:]))


def build_world():
    """The seeded world with the course built, its terrain spot found, saved as the snapshot every run restores."""
    folder = WORK / "world-build"
    shutil.rmtree(folder, ignore_errors=True)
    port = lab.free_port()
    server_folder(folder, port)
    server = Server(server_command(), folder, "world-build")
    try:
        server.wait_for(r"Done \(", timeout=900)
        for rule in GAMERULES:
            server.console(f"gamerule {rule}")
        server.console("time set 6000")
        server.console("weather clear")
        width = routes.COLUMNS * 8 - 1
        depth = (len(routes.ORDER) + routes.COLUMNS - 1) // routes.COLUMNS * 40 - 1
        server.console(f"forceload add -48 -32 {width + 48} {depth}")
        deadline = time.time() + 900
        while True:
            try:
                ask(server, f"execute if loaded {width} 100 {depth} if loaded -48 100 -32 run say course-loaded",
                    r"course-loaded", timeout=3)
                break
            except RuntimeError:
                if time.time() > deadline:
                    raise
        for command in routes.course_commands():
            server.console(command)
        spot = None
        for x, z in TERRAIN_CANDIDATES:
            try:
                ask(server, f"execute positioned {x} 0 {z} positioned over motion_blocking_no_leaves "
                            f"unless block ~ ~-1 ~ #minecraft:replaceable unless block ~ ~-1 ~ minecraft:water "
                            f"unless block ~ ~-1 ~ minecraft:lava run say spot-ok {x} {z}", r"spot-ok", timeout=5)
            except RuntimeError:
                continue
            server.console(f"execute positioned {x} 0 {z} positioned over motion_blocking_no_leaves run "
                           f"summon minecraft:marker ~ ~ ~ {{Tags:[\"pp_spot\"]}}")
            line = ask(server, "data get entity @e[tag=pp_spot,limit=1] Pos", r"entity data: \[")
            y = float(re.search(r"\[[^,]+, ([-\d.]+)d", line).group(1))
            server.console("kill @e[tag=pp_spot]")
            spot = (x, y, z)
            break
        if spot is None:
            raise RuntimeError("no dry terrain spot among TERRAIN_CANDIDATES")
        server.console("forceload remove all")
        ask(server, "save-all flush", r"Saved the game", timeout=300)
    finally:
        server.stop()
    shutil.rmtree(SNAPSHOT, ignore_errors=True)
    shutil.copytree(folder / "world", SNAPSHOT)
    (WORK / "course.json").write_text(json.dumps({"seed": SEED, "terrain": spot, "routes": routes.ORDER}))
    print(f"world snapshot: {SNAPSHOT} (terrain spot {spot})")


class Display:
    """A fresh Xvfb display with GLX, never the desktop's."""

    def __init__(self, log):
        read, write = os.pipe()
        self.process = subprocess.Popen(
            ["Xvfb", "-displayfd", str(write), "-screen", "0", "1280x800x24", "-nolisten", "tcp", "+extension",
             "GLX", "-noreset"], pass_fds=[write], stdout=log, stderr=subprocess.STDOUT)
        os.close(write)
        with os.fdopen(read) as f:
            self.name = f":{f.readline().strip()}"

    def env(self, env):
        env = dict(env)
        env.pop("WAYLAND_DISPLAY", None)
        env["DISPLAY"] = self.name
        env["XDG_SESSION_TYPE"] = "x11"
        env["__GLX_VENDOR_LIBRARY_NAME"] = "mesa"
        env["__EGL_VENDOR_LIBRARY_FILENAMES"] = "/usr/share/glvnd/egl_vendor.d/50_mesa.json"
        if LVP.exists():
            env["VK_ICD_FILENAMES"] = str(LVP)
        return env

    def screenshot(self, path):
        subprocess.run(["import", "-display", self.name, "-window", "root", str(path)], capture_output=True,
                       timeout=30)

    def close(self):
        self.process.terminate()
        try:
            self.process.wait(10)
        except subprocess.TimeoutExpired:
            self.process.kill()


def game_window(display_name, timeout=300):
    """The client's window on the display, given the input focus and the pointer, once it is mapped."""
    from Xlib import X, display as xdisplay
    d = xdisplay.Display(display_name)
    deadline = time.time() + timeout
    while time.time() < deadline:
        for w in d.screen().root.query_tree().children:
            try:
                attrs, geo = w.get_attributes(), w.get_geometry()
            except Exception:
                continue
            if attrs.map_state == X.IsViewable and geo.width >= 400 and geo.height >= 300:
                w.set_input_focus(X.RevertToParent, X.CurrentTime)
                d.screen().root.warp_pointer(geo.x + geo.width // 2, geo.y + geo.height // 2)
                d.sync()
                return d, w
        time.sleep(0.5)
    raise RuntimeError(f"no client window on {display_name} in {timeout} s")


def options_file(folder):
    folder.mkdir(parents=True, exist_ok=True)
    (folder / "options.txt").write_text("".join(f"{k}:{v}\n" for k, v in OPTIONS.items()))


def start_vanilla(run, address, display, log):
    import vanilla_client
    args = argparse.Namespace(name=PLAYER, join=address, world=None, game_dir=run / "client", assets=None,
                              heap="2G", width=routes.WIDTH, height=routes.HEIGHT, lan_port=None, backend=None)
    options_file(args.game_dir)
    vanilla_client.write_options(args.game_dir, OPTIONS)
    env = display.env(vanilla_client.environment())
    return subprocess.Popen(vanilla_client.command(args), cwd=args.game_dir, env=env, stdin=subprocess.DEVNULL,
                            stdout=log, stderr=subprocess.STDOUT)


def start_sunrise(run, address, display, log, bins):
    game = run / "client"
    options_file(game)
    home = run / "home"
    for d in ("home", "launcher", "data", "config"):
        (run / d).mkdir(parents=True, exist_ok=True)
    env = lab.game_env(SUNRISE_HOME=home, SUNRISE_LAUNCHER_HOME=run / "launcher", XDG_DATA_HOME=run / "data",
                       XDG_CONFIG_HOME=run / "config", SUNRISE_TELEMETRY_URL="off")
    env = display.env(env)
    command = [str(lab.binary(bins, "sunrise-client")), "--resources", str(lab.LOCAL / "sunrise-client" / lab.VERSION),
               "--server", address, "--name", PLAYER, "--width", str(routes.WIDTH), "--height", str(routes.HEIGHT)]
    return subprocess.Popen(command, cwd=game, env=env, stdin=subprocess.DEVNULL, stdout=log,
                            stderr=subprocess.STDOUT)


def start_opponent(run, port, bins):
    """The PvP routes' opponent: one sunrise-bots player that stands where the console puts it."""
    log = open(run / "opponent.log", "w")
    command = [str(lab.binary(bins, "sunrise-bots")), "--server", f"127.0.0.1:{port}", "--bots", "1",
               "--behaviour", "stand", "--name", routes.OPPONENT[:-1], "--seconds", "100000", "--view-distance", "4"]
    return subprocess.Popen(command, stdin=subprocess.DEVNULL, stdout=log, stderr=subprocess.STDOUT)


def wait_in_game(counts, process, timeout=600):
    """Until the client ends its first 40 ticks in the play state."""
    deadline = time.time() + timeout
    while counts.update().ticks < 40:
        if process.poll() is not None:
            raise RuntimeError(f"the client exited ({process.returncode}) before playing")
        if time.time() > deadline:
            raise RuntimeError("the client never reached play")
        time.sleep(0.5)


def scaled(events, scale):
    """Relative mouse moves times `scale`, the remainder carried, for a diagnostic pass past a turning difference."""
    out, carry = [], [0.0, 0.0]
    for e in events:
        if e["type"] == "rel" and scale != 1:
            carry = [carry[0] + e["dx"] * scale, carry[1] + e["dy"] * scale]
            dx, dy = int(carry[0]), int(carry[1])
            carry = [carry[0] - dx, carry[1] - dy]
            if dx or dy:
                out.append(dict(e, dx=dx, dy=dy))
        else:
            out.append(e)
    return out


def run_once(client, index, recording, bins, label=None, mouse_scale=1.0):
    """One client, one pass over every route of the recording, on a restored world."""
    course = json.loads((WORK / "course.json").read_text())
    run = WORK / "runs" / f"{label or client}-{index}"
    shutil.rmtree(run, ignore_errors=True)
    run.mkdir(parents=True)
    port, listen = lab.free_port(), lab.free_port()
    server_folder(run / "server", port)
    shutil.copytree(SNAPSHOT, run / "server" / "world")
    capture = run / "capture.jsonl"
    capture.touch()
    logs = {name: open(run / f"{name}.log", "w") for name in ("xvfb", "client")}
    server = Server(server_command(), run / "server", "server")
    display = stop = process = opponent = None
    try:
        server.wait_for(r"Done \(", timeout=900)
        stop = net_capture.start_background(listen, f"127.0.0.1:{port}", capture, full=True)
        if any(routes.ROUTES.get(name, {}).get("opponent") for name, _ in input_recording.routes(recording)):
            opponent = start_opponent(run, port, bins)
            server.wait_for(rf"{routes.OPPONENT} joined the game", timeout=120)
        display = Display(logs["xvfb"])
        address = f"127.0.0.1:{listen}"
        process = (start_vanilla(run, address, display, logs["client"]) if client == "vanilla"
                   else start_sunrise(run, address, display, logs["client"], bins))
        game_window(display.name)
        counts = ClientCounts(capture)
        wait_in_game(counts, process)
        game_window(display.name, timeout=10)
        injector = input_recording.Injector(display.name)
        display.screenshot(run / "joined.png")
        played = []
        for name, events in input_recording.routes(recording):
            if process.poll() is not None:
                raise RuntimeError(f"the client exited ({process.returncode}) before {name}")
            injector.play([{"t": 0.0, "type": "key", "code": "KEY_1", "down": True},
                           {"t": 0.08, "type": "key", "code": "KEY_1", "down": False}])
            commands = (routes.reset_commands(name, course["terrain"]) if name in routes.ROUTES
                        else [f"tp {PLAYER} {PLAYER}"])
            accepted = counts.update().accepted + sum(c.startswith(f"tp {PLAYER} ") for c in commands)
            for command in commands:
                server.console(command)
            settle(counts, accepted)
            timers = [threading.Timer(t, server.console, [c]) for t, c in routes.ROUTES.get(name, {}).get("during", [])]
            for timer in timers:
                timer.start()
            seconds = injector.play(scaled(events, mouse_scale))
            for timer in timers:
                timer.join()
            time.sleep(1.0)
            display.screenshot(run / f"{name}.png")
            played.append({"route": name, "seconds": round(seconds, 2)})
            print(f"  {run.name} {name}: {seconds:.1f} s", flush=True)
        (run / "played.json").write_text(json.dumps(played))
    finally:
        if opponent and opponent.poll() is None:
            opponent.kill()
            opponent.wait()
        if process and process.poll() is None:
            process.terminate()
            try:
                process.wait(20)
            except subprocess.TimeoutExpired:
                process.kill()
        if stop:
            stop.set()
        if display:
            display.close()
        server.stop()
        for f in logs.values():
            f.close()
    return run


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="command", required=True)
    sub.add_parser("world")
    for name in ("run", "all"):
        p = sub.add_parser(name)
        if name == "run":
            p.add_argument("--client", choices=["vanilla", "sunrise"], required=True)
        p.add_argument("--runs", type=int, default=3)
        p.add_argument("--first", type=int, default=1, help="the first run's number")
        p.add_argument("--route", action="append", help="only these scripted routes")
        p.add_argument("--recording", type=Path, help="an input recording instead of the scripted routes")
        p.add_argument("--bin", type=Path, default=lab.ROOT / "target/fast", help="sunrise-client's folder")
        p.add_argument("--label", help="the runs' folder name instead of the client's (runs/<label>-N)")
        p.add_argument("--mouse-scale", type=float, default=1.0,
                       help="diagnostic: scale relative mouse moves, to look past a known turning difference")
    d = sub.add_parser("diff")
    d.add_argument("--out", type=Path, default=WORK / "report.json")
    args = parser.parse_args()
    if args.command == "world":
        build_world()
        return
    if args.command in ("run", "all"):
        if args.command == "all" or not (WORK / "course.json").exists():
            build_world()
        recording = (input_recording.read(args.recording) if args.recording
                     else routes.recording(args.route or routes.ORDER))
        clients = [args.client] if args.command == "run" else ["vanilla", "sunrise"]
        for client in clients:
            for i in range(args.first, args.first + args.runs):
                print(f"{client} run {i}", flush=True)
                run_once(client, i, recording, args.bin, args.label, args.mouse_scale)
        if args.command == "run":
            return
    import client_packet_diff
    report = client_packet_diff.report_runs(WORK / "runs")
    (args.out if args.command == "diff" else WORK / "report.json").write_text(json.dumps(report, indent=1))
    print(client_packet_diff.describe(report))


if __name__ == "__main__":
    main()
