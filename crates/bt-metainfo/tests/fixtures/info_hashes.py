"""Print the info-hashes of .torrent files, independently of the Rust code.

Usage: python -I info_hashes.py FILE...

For each file prints the v1 info-hash (SHA-1 of the raw `info` bytes) and,
when `info` has `meta version` 2, the v2 info-hash (SHA-256 of the same
bytes). Uses only the standard library.
"""

import hashlib
import sys


def skip(data, i):
    """Return the index just past the bencoded value starting at i."""
    c = data[i:i + 1]
    if c == b"i":
        return data.index(b"e", i) + 1
    if c in (b"l", b"d"):
        i += 1
        while data[i:i + 1] != b"e":
            i = skip(data, i)
        return i + 1
    if c.isdigit():
        colon = data.index(b":", i)
        return colon + 1 + int(data[i:colon])
    raise ValueError(f"bad bencode at byte {i}")


def dict_items(data, i):
    """Yield (key, value_start, value_end) for the dict starting at i."""
    assert data[i:i + 1] == b"d", f"expected dict at byte {i}"
    i += 1
    while data[i:i + 1] != b"e":
        key_end = skip(data, i)
        colon = data.index(b":", i)
        key = data[colon + 1:key_end]
        value_end = skip(data, key_end)
        yield key, key_end, value_end
        i = value_end


def info_bytes(data):
    for key, start, end in dict_items(data, 0):
        if key == b"info":
            return data[start:end]
    raise ValueError("no info dictionary")


def meta_version(info):
    for key, start, end in dict_items(info, 0):
        if key == b"meta version":
            return int(info[start + 1:end - 1])
    return None


def main(paths):
    for path in paths:
        with open(path, "rb") as f:
            info = info_bytes(f.read())
        print(path)
        print("  v1:", hashlib.sha1(info).hexdigest())
        if meta_version(info) == 2:
            print("  v2:", hashlib.sha256(info).hexdigest())


if __name__ == "__main__":
    main(sys.argv[1:])
