#!/usr/bin/env python3
"""Lists a wasm file's import section: `imports.py FILE [--names]` prints counts per module."""
import collections
import sys


def leb(b, i):
    r = s = 0
    while True:
        c = b[i]
        i += 1
        r |= (c & 0x7F) << s
        s += 7
        if not c & 0x80:
            return r, i


b = open(sys.argv[1], "rb").read()
i = 8
imports = []
while i < len(b):
    sid = b[i]
    size, j = leb(b, i + 1)
    end = j + size
    if sid == 2:
        count, k = leb(b, j)
        for _ in range(count):
            n, k = leb(b, k)
            mod = b[k : k + n].decode()
            k += n
            n, k = leb(b, k)
            name = b[k : k + n].decode()
            k += n
            kind = b[k]
            k += 1
            if kind == 0:
                _, k = leb(b, k)
            elif kind == 1:
                k += 1
                flag = b[k]
                k += 1
                _, k = leb(b, k)
                if flag & 1:
                    _, k = leb(b, k)
            elif kind == 2:
                flag = b[k]
                k += 1
                _, k = leb(b, k)
                if flag & 1:
                    _, k = leb(b, k)
            elif kind == 3:
                k += 2
            imports.append((mod, name, kind))
    if sid == 7 and "--exports" in sys.argv:
        count, k = leb(b, j)
        for _ in range(count):
            n, k = leb(b, k)
            print("  export", b[k : k + n].decode()[:110])
            k += n + 1
            _, k = leb(b, k)
    i = end
by = collections.Counter(m for m, _, _ in imports)
print(sys.argv[1].split("/")[-1], "size", len(b), "imports", dict(by))
if "--names" in sys.argv:
    for m, n, k in imports:
        if m == "sunrise":
            print("  ", len(n), n[:100])
if "--check-names" in sys.argv:
    import hashlib
    want = set()
    for k, size in enumerate((80, 1000, 20000)):
        p = "call:ServerHost::long_method#"
        want.add(p + (hashlib.sha256(f"m{k}".encode()).hexdigest()[:32] * 1000)[: size - len(p)])
    got = {n for m, n, _ in imports if m == "sunrise"}
    print("exact names equal:", got == want)
