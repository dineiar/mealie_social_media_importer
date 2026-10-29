#!/usr/bin/env python3
"""Run one gallery-dl or yt-dlp command against a private copy of a cookies file and record what happened.

    capture.py --run-dir DIR [--cookies FILE] [--bin-dir DIR] -- gallery-dl --cookies '{COOKIES}' -D '{OUT}' ... -- URL

In the command, {COOKIES} becomes the run's copy of the cookies file and {OUT} becomes DIR/out.
The script deletes DIR first. DIR receives cmd.txt, stdout.txt, stderr.txt, exit.txt, files.txt, version.txt and out/. With --cookies it
also receives cookies-before.txt, cookies-after.txt and cookies-diff.txt. Those list cookie names, expiry
and value length, never values. The copy itself stays in DIR with mode 600, so keep DIR out of git.
A tool that replaces the cookies file, as gallery-dl does, resets its mode, so the script sets 600 again.
"""

import argparse
import hashlib
import os
import shlex
import shutil
import subprocess
import sys
from typing import NamedTuple

TIMEOUT_SECS = 180
VERSION_TIMEOUT_SECS = 30


class Cookie(NamedTuple):
    expiry: str
    value: str


def read_jar(path):
    with open(path, "rb") as f:
        data = f.read()
    jar = {}
    for line in data.decode().splitlines():
        line = line.removeprefix("#HttpOnly_")
        if not line or line.startswith("#"):
            continue
        fields = line.split("\t")
        if len(fields) >= 7:
            jar[(fields[0], fields[5])] = Cookie(expiry=fields[4], value=fields[6])
    return hashlib.sha256(data).hexdigest(), jar


def summary(digest, jar):
    rows = [f"{domain}|{name}|exp={c.expiry}|valuelen={len(c.value)}" for (domain, name), c in sorted(jar.items())]
    return f"sha256={digest}\nlines={len(rows)}\n" + "\n".join(rows) + "\n"


def diff(before, after):
    rows = []
    for key in sorted(set(before) | set(after)):
        old, new = before.get(key), after.get(key)
        if old is None:
            rows.append(f"{key[1]}: added (expires={'session' if new.expiry == '0' else 'set'})")
        elif new is None:
            rows.append(f"{key[1]}: removed")
        else:
            changes = [label for label, differs in (("value changed", old.value != new.value), ("expiry changed", old.expiry != new.expiry)) if differs]
            rows.append(f"{key[1]}: {', '.join(changes) or 'unchanged'}")
    return "\n".join(rows) + "\n"


def write(run_dir, name, data):
    mode = "wb" if isinstance(data, bytes) else "w"
    with open(os.path.join(run_dir, name), mode) as f:
        f.write(data)


def substitute(arg, cookies, out):
    return arg.replace("{COOKIES}", cookies).replace("{OUT}", out)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--run-dir", required=True)
    ap.add_argument("--cookies")
    ap.add_argument("--bin-dir")
    ap.add_argument("command", nargs=argparse.REMAINDER)
    args = ap.parse_args()
    command = args.command[1:] if args.command[:1] == ["--"] else args.command
    if not command:
        sys.exit("no command given")

    run_dir = os.path.abspath(args.run_dir)
    shutil.rmtree(run_dir, ignore_errors=True)
    out_dir = os.path.join(run_dir, "out")
    os.makedirs(out_dir)
    os.chmod(run_dir, 0o700)

    jar_copy = os.path.join(run_dir, "cookies.txt")
    if args.cookies:
        shutil.copyfile(args.cookies, jar_copy)
        os.chmod(jar_copy, 0o600)
        digest, before = read_jar(jar_copy)
        write(run_dir, "cookies-before.txt", summary(digest, before))

    tool = os.path.join(args.bin_dir, command[0]) if args.bin_dir else command[0]
    argv = [tool] + [substitute(a, jar_copy, out_dir) for a in command[1:]]
    shown = [command[0]] + [substitute(a, "<cookies>", "<out>") for a in command[1:]]
    write(run_dir, "cmd.txt", shlex.join(shown) + "\n")
    try:
        version = subprocess.run([tool, "--version"], capture_output=True, text=True, timeout=VERSION_TIMEOUT_SECS).stdout.strip()
    except subprocess.TimeoutExpired:
        version = "timeout"
    write(run_dir, "version.txt", version + "\n")

    try:
        done = subprocess.run(argv, capture_output=True, timeout=TIMEOUT_SECS, cwd=run_dir)
        code, stdout, stderr = done.returncode, done.stdout, done.stderr
    except subprocess.TimeoutExpired as exc:
        code, stdout, stderr = "timeout", exc.stdout or b"", exc.stderr or b""
    write(run_dir, "stdout.txt", stdout)
    write(run_dir, "stderr.txt", stderr)
    write(run_dir, "exit.txt", f"{code}\n")

    rows = []
    for root, _, names in sorted(os.walk(out_dir)):
        paths = [os.path.join(root, n) for n in sorted(names)]
        rows += [f"{os.path.relpath(p, out_dir)}\t{os.path.getsize(p)}" for p in paths]
    write(run_dir, "files.txt", "\n".join(rows) + ("\n" if rows else ""))

    if args.cookies:
        os.chmod(jar_copy, 0o600)
        digest, after = read_jar(jar_copy)
        write(run_dir, "cookies-after.txt", summary(digest, after))
        write(run_dir, "cookies-diff.txt", diff(before, after))
    print(f"{command[0]} exit={code} stdout={len(stdout)}B stderr={len(stderr)}B files={len(rows)}")


if __name__ == "__main__":
    main()
