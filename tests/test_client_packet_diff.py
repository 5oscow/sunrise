"""tools/client_packet_diff.py keeps only the differences every run reproduces, whatever tick real-time input
landed on; tools/input_recording.py cuts a recording at its markers."""
from pathlib import Path
import importlib.util
import unittest

TOOLS = Path(__file__).resolve().parents[1] / 'tools'


def load(name):
    spec = importlib.util.spec_from_file_location(name, TOOLS / f'{name}.py')
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


diff = load('client_packet_diff')
recording = load('input_recording')


def tick(*packets):
    return {'packets': [*packets, ('client_tick_end', {})], 'frame': [], 'misplaced': []}


def move(z, yaw=0.0):
    return ('move_player_pos_rot', {'x': 0.5, 'y': 101.0, 'z': z, 'yaw': yaw, 'pitch': 0.0, 'on_ground': True,
                                    'horizontal_collision': False})


def run(name, ticks):
    return {'run': name, 'routes': {'walk': diff.features(ticks)}, 'ticks': {'walk': ticks}, 'keep_alive': []}


def kinds(rep):
    return sorted({(f['kind'], f.get('side')) for f in rep['findings']})


class ClientPacketDiffTests(unittest.TestCase):
    def test_the_same_packets_a_tick_later_are_no_finding(self):
        walk = [tick(), tick(('player_input', {'keys': ['forward']}), move(0.6)), tick(move(0.8)), tick(move(1.0))]
        late = [tick(), tick(), tick(('player_input', {'keys': ['forward']}), move(0.6)), tick(move(0.8)),
                tick(move(1.0))]
        rep = diff.report([run('vanilla-1', walk), run('vanilla-2', late)],
                          [run('sunrise-1', late), run('sunrise-2', walk)])
        self.assertEqual(rep['findings'], [])

    def test_a_tick_shape_only_one_client_sends_in_every_run_is_a_finding(self):
        vanilla = [tick(('swing', {'raw': '00'}), ('attack', {'entity': 'e1'}))]
        sunrise = [tick(('attack', {'entity': 'e1'}), ('swing', {'raw': '00'}))]
        rep = diff.report([run('vanilla-1', vanilla), run('vanilla-2', vanilla)],
                          [run('sunrise-1', sunrise), run('sunrise-2', sunrise)])
        self.assertIn(('shapes', 'only_sunrise'), kinds(rep))
        self.assertIn(('shapes', 'only_vanilla'), kinds(rep))

    def test_a_difference_one_run_lacks_is_noise(self):
        vanilla = [tick(move(1.0))]
        odd = [tick(move(1.0)), tick(('swing', {'raw': '00'}))]
        rep = diff.report([run('vanilla-1', vanilla), run('vanilla-2', vanilla)],
                          [run('sunrise-1', odd), run('sunrise-2', vanilla)])
        self.assertEqual(rep['findings'], [])

    def test_block_sequences_compare_as_steps_not_absolute_numbers(self):
        def digs(first):
            return [tick(('player_action', {'action': 0, 'pos': [0, 101, 2], 'direction': 2, 'sequence': first})),
                    tick(('player_action', {'action': 2, 'pos': [0, 101, 2], 'direction': 2,
                                            'sequence': first + 1}))]
        rep = diff.report([run('vanilla-1', digs(4)), run('vanilla-2', digs(9))],
                          [run('sunrise-1', digs(1)), run('sunrise-2', digs(30))])
        self.assertEqual(rep['findings'], [])

    def test_an_end_yaw_outside_the_other_clients_range_is_a_finding(self):
        rep = diff.report([run('vanilla-1', [tick(move(0, 0.0)), tick(move(0, 59.7))])] * 2,
                          [run('sunrise-1', [tick(move(0, 0.0)), tick(move(0, 119.7))])] * 2)
        ends = [f for f in rep['findings'] if f['kind'] == 'end_yaw']
        self.assertEqual(ends[0]['vanilla'], [59.7, 59.7])
        self.assertEqual(ends[0]['sunrise'], [119.7, 119.7])


class InputRecordingTests(unittest.TestCase):
    def test_a_recording_is_cut_at_its_markers_with_times_from_each_marker(self):
        events = [{'t': 0.0, 'type': 'marker', 'route': 'walk'},
                  {'t': 1.0, 'type': 'key', 'code': 'KEY_W', 'down': True},
                  {'t': 5.0, 'type': 'marker', 'route': 'jump'},
                  {'t': 5.5, 'type': 'key', 'code': 'KEY_SPACE', 'down': True}]
        cut = recording.routes(events)
        self.assertEqual([name for name, _ in cut], ['walk', 'jump'])
        self.assertEqual([e['t'] for e in cut[1][1]], [0.0, 0.5])

    def test_a_marker_types_the_chat_command_and_ends_before_the_route_starts(self):
        keys = [e['code'] for e in recording.marker_events(0.0) if e['down']]
        self.assertEqual(keys[0], 'KEY_T')
        self.assertEqual(keys[-1], 'KEY_ENTER')
        typed = ''.join({'KEY_SLASH': '/', 'KEY_MINUS': '-'}.get(k, k[4:].lower()) for k in keys[1:-1])
        self.assertEqual(typed, '/wire-mark')
        self.assertLess(max(e['t'] for e in recording.marker_events(0.0)), recording.MARKER_SECONDS)


if __name__ == '__main__':
    unittest.main()
