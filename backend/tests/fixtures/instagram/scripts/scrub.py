#!/usr/bin/env python3
"""Turn recorded gallery-dl and yt-dlp runs into scrubbed, replayable test fixtures.

Each run is a directory written by capture.py. The script reads the real identifiers out
of the recorded data itself, so it holds none. Every handle, name, caption, shortcode,
numeric id, post date and CDN URL is replaced by a synthetic value of the same type and shape,
and signed query strings are dropped. A run's cookies-diff.txt becomes `cookie_changes` in
case.json, which names cookies and never holds values. If any real identifier survives in the
output, the script exits with an error and writes nothing.

The leak check compares against what the scrubber learned, so it only knows values that a
key rule or a caption registered. It skips handles and names under four characters, and
the word "instagram". After each case the script prints the string values that passed through
unchanged. Read that list. A value that identifies a person or a post means a new key
needs a rule.

    scrub.py --case reel --note "..." --out fixtures/reel \
        --url https://www.instagram.com/reels/<real code>/ \
        --tool-version gallery-dl=1.32.18 --tool-version yt-dlp=2026.10.26 \
        --run gallery-dl.dump-json=<run dir> --run gallery-dl.download=<run dir> \
        --run yt-dlp.dump-json=<run dir> --run yt-dlp.download=<run dir>
"""

import argparse
import json
import os
import re
import shlex
import sys
from pathlib import Path
from urllib.parse import urlsplit

ALPHABET = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_"
FIXED_DATE = "2026-01-02 03:04:05"
DATE_RE = re.compile(r"^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}$")
TAG_RE = re.compile(r"#\w+")
DIGITS_RE = re.compile(r"\d{7,}")
URL_PATTERN = r"https?://[^\s'\"<>]+"
CODE_KEYS = {"post_shortcode", "sidecar_shortcode", "shortcode"}
ID_KEYS = {"owner_id", "pk", "id", "pk_id", "strong_id__", "account_id", "user_id", "location_id"}
VIDEO_EXT = {"mp4", "mov", "webm"}
IMAGE_EXT = {"jpg", "jpeg", "png", "webp", "heic"}
GENERIC_WORDS = {"instagram"}
CDN_HOSTS = ("cdninstagram.com", "fbcdn.net")
OWN_HOSTS = ("instagram.com", "fbcdn.net")
POST_PREFIXES = {"p", "reel", "reels", "tv"}
RUN_KINDS = ("gallery-dl.dump-json", "gallery-dl.download", "yt-dlp.dump-json", "yt-dlp.download")
COOKIE_CHANGES = {"added": "added", "removed": "removed", "expiry changed": "expiry_changed", "value changed": "value_changed"}


def stem(path):
    return os.path.splitext(os.path.basename(path))[0]


def dumps(obj):
    return json.dumps(obj, indent=2, ensure_ascii=False) + "\n"


def id_from_code(code):
    n = 0
    for ch in code:
        n = n * 64 + ALPHABET.index(ch)
    return n


class Identity:
    def __init__(self):
        self.codes, self.digits, self.handles, self.names, self.cdn = {}, {}, {}, {}, {}
        self.real_text = set()
        self.unchanged = set()
        self.next_digits = 0
        self.by_key = {
            **dict.fromkeys(CODE_KEYS, self.code),
            "username": self.handle,
            "fullname": self.name,
            "full_name": self.name,
        }

    def code(self, real):
        if real not in self.codes:
            n = len(self.codes) + 1
            width = max(len(real) - 6, 1)
            synthetic = f"Dsynth{n:0{width}d}"[: len(real)]
            self.codes[real] = synthetic
            self.real_text.add(real)
            try:
                self.digits[str(id_from_code(real))] = str(id_from_code(synthetic))
            except ValueError:
                pass
        return self.codes[real]

    def digit_run(self, real):
        if real not in self.digits:
            self.next_digits += 1
            self.digits[real] = f"9{self.next_digits:0{max(len(real) - 1, 1)}d}"[: len(real)]
            self.real_text.add(real)
        return self.digits[real]

    def alias(self, table, template, real):
        if not real:
            return real
        if real not in table:
            table[real] = template.format(len(table) + 1)
            self.track(real)
        return table[real]

    def handle(self, real):
        return self.alias(self.handles, "synthetic_user_{:02d}", real)

    def name(self, real):
        return self.alias(self.names, "Synthetic Author {:02d}", real)

    def track(self, real):
        if len(real) >= 4 and real.lower() not in GENERIC_WORDS:
            self.real_text.add(real)

    def caption(self, real):
        if not real:
            return real
        self.real_text.update(line.strip() for line in real.splitlines() if len(line.strip()) >= 12)
        real_tags = sorted(set(TAG_RE.findall(real)))
        self.real_text.update(real_tags)
        text = "Synthetic caption for a fixture post."
        if any(ord(c) >= 0x1F000 for c in real):
            text += " \U0001F34B"
        if real.count("\n") >= 2:
            text += "\n\nSecond synthetic paragraph."
        if real_tags:
            accented = any(not t.isascii() for t in real_tags)
            tags = [f"#synthtag{i:02d}" for i in range(1, len(real_tags) + 1)]
            if accented:
                tags[-1] = "#sintético"
            text += "\n\n" + " ".join(tags)
        return text

    def url(self, real):
        parts = urlsplit(real)
        host = parts.netloc.lower()
        if host.endswith(CDN_HOSTS):
            if parts.path not in self.cdn:
                self.real_text.add(stem(parts.path))
                ext = os.path.splitext(parts.path)[1]
                self.cdn[parts.path] = f"https://scontent.cdninstagram.com/v/synthetic/{len(self.cdn) + 1:03d}{ext}"
            return self.cdn[parts.path]
        if host.endswith("instagram.com"):
            raw = parts.path.strip("/").split("/")
            segments = [self.segment(prev, seg) for prev, seg in zip(["", *raw], raw)]
            trailing = "/" if parts.path.endswith("/") else ""
            return f"https://www.instagram.com/{'/'.join(segments)}{trailing}"
        return "https://example.invalid/synthetic"

    def segment(self, prev, seg):
        if prev in POST_PREFIXES | {"share"} and seg not in POST_PREFIXES:
            return self.code(seg)
        if seg in self.handles:
            return self.handles[seg]
        if DIGITS_RE.fullmatch(seg):
            return self.digit_run(seg)
        return seg

    def text(self, value):
        if value.startswith(("http://", "https://")):
            return self.url(value)
        alternatives = [rf"(?P<url>{URL_PATTERN})"]
        if self.codes:
            known = "|".join(re.escape(c) for c in sorted(self.codes, key=len, reverse=True))
            alternatives.append(rf"(?P<code>(?<![A-Za-z0-9_-])(?:{known})(?![A-Za-z0-9_-]))")
        alternatives.append(rf"(?P<num>{DIGITS_RE.pattern})")

        def replace(match):
            found = match.group(0)
            if match.lastgroup == "url":
                return self.url(found) if urlsplit(found).netloc.lower().endswith(OWN_HOSTS) else found
            if match.lastgroup == "code":
                return self.codes[found]
            return self.digit_run(found)

        return re.sub("|".join(alternatives), replace, value)

    def learn(self, value, key=""):
        if isinstance(value, dict):
            for k, v in value.items():
                self.learn(v, k)
        elif isinstance(value, list):
            for v in value:
                self.learn(v, key)
        elif isinstance(value, str):
            if key in self.by_key:
                self.by_key[key](value)
            elif key == "filename" and value:
                self.real_text.add(value)

    def scrub(self, value, key="", path=()):
        if isinstance(value, dict):
            out = {k: self.scrub(v, k, path + (k,)) for k, v in value.items()}
            if "description" in value and "tags" in value:
                out["tags"] = sorted(set(TAG_RE.findall(out["description"])))
            elif "tags" in value and isinstance(value["tags"], list):
                self.real_text.update(value["tags"])
                out["tags"] = [f"#synthtag{i:02d}" for i in range(1, len(value["tags"]) + 1)]
            if "filename" in value and "extension" in value:
                video = out.get("video_url") if value["extension"] in VIDEO_EXT else None
                url = video or out.get("display_url")
                if url:
                    out["filename"] = stem(urlsplit(url).path)
            return out
        if isinstance(value, list):
            return [self.scrub(v, key, path) for v in value]
        if isinstance(value, bool):
            return False if "friendship_status" in path else value
        if isinstance(value, int):
            if key == "likes":
                return 42
            if key in ID_KEYS or abs(value) >= 1_000_000:
                return int(self.digit_run(str(value)))
            return value
        if isinstance(value, str):
            if key in self.by_key:
                return self.by_key[key](value)
            if key == "description":
                return self.caption(value)
            if key in {"date", "post_date"} and DATE_RE.match(value):
                return FIXED_DATE
            if key == "audio_title":
                return value if value == "Original audio" else "Synthetic track"
            if key == "audio_artist":
                return "Synthetic Artist"
            if key in ID_KEYS and value.isdigit():
                return self.digit_run(value)
            return self.text(value)
        return value

    def scrub_document(self, doc):
        out = self.scrub(doc)
        self.note_unchanged(doc, out)
        return out

    def note_unchanged(self, raw, out, key=""):
        if isinstance(raw, dict):
            for k, v in raw.items():
                self.note_unchanged(v, out[k], k)
        elif isinstance(raw, list):
            for r, o in zip(raw, out):
                self.note_unchanged(r, o, key)
        elif isinstance(raw, str) and raw == out:
            self.unchanged.add(f"{key}={raw[:40]}")

    def scrub_text(self, text, directory):
        return self.text(text.replace(directory, "{dir}")).rstrip("\n") + "\n"

    def leaks(self, serialized):
        found = [s for s in self.real_text if s and s in serialized]
        found += re.findall(r"https?://[^\s\"']*(?:instagram\.com|fbcdn\.net)/[^\s\"']*\?[^\s\"']*", serialized)
        return found


def read(path):
    path = Path(path)
    return path.read_text(encoding="utf-8") if path.exists() else ""


def parse_command(run_dir):
    parts = shlex.split(read(os.path.join(run_dir, "cmd.txt")))
    return [os.path.basename(parts[0])] + parts[1:]


def kind_of(name):
    ext = name.rsplit(".", 1)[-1].lower()
    return "json" if ext == "json" else "video" if ext in VIDEO_EXT else "image" if ext in IMAGE_EXT else "file"


def cookie_changes(directory):
    text = read(os.path.join(directory, "cookies-diff.txt"))
    if not text:
        return None
    changes = {key: [] for key in COOKIE_CHANGES.values()}
    for line in text.splitlines():
        name, _, what = line.partition(": ")
        for label, key in COOKIE_CHANGES.items():
            if label in what:
                changes[key].append(name)
    return changes


def load_run(directory):
    raw_exit = read(os.path.join(directory, "exit.txt")).strip()
    return {
        "dir": directory,
        "out": os.path.join(directory, "out"),
        "stdout": read(os.path.join(directory, "stdout.txt")),
        "stderr": read(os.path.join(directory, "stderr.txt")),
        "exit": int(raw_exit) if raw_exit.lstrip("-").isdigit() else raw_exit,
        "cookie_changes": cookie_changes(directory),
    }


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--case", required=True)
    ap.add_argument("--note", default="")
    ap.add_argument("--out", required=True)
    ap.add_argument("--url", action="append", default=[], help="real input URL used by the runs")
    ap.add_argument("--tool-version", action="append", default=[])
    ap.add_argument("--run", action="append", default=[])
    args = ap.parse_args()

    runs = {}
    for spec in args.run:
        kind, _, directory = spec.partition("=")
        if kind not in RUN_KINDS:
            sys.exit(f"unknown run kind {kind}")
        runs[kind] = load_run(directory)

    ident = Identity()
    for url in args.url:
        ident.code(urlsplit(url).path.strip("/").split("/")[-1])
    documents = {}
    for kind, run in runs.items():
        if kind == "gallery-dl.dump-json" and run["stdout"].strip():
            documents[(kind, "stdout")] = json.loads(run["stdout"])
        if kind == "gallery-dl.download" and os.path.isdir(run["out"]):
            for name in sorted(os.listdir(run["out"])):
                if name.endswith(".json"):
                    documents[(kind, name)] = json.loads(read(os.path.join(run["out"], name)))
    for doc in documents.values():
        ident.learn(doc)

    files, case_runs = {}, {}

    def put(name, body):
        files[name] = body
        return name

    for kind in RUN_KINDS:
        if kind not in runs:
            continue
        run = runs[kind]
        cmd = [a.replace("<out>", "<dir>") for a in parse_command(run["dir"])]
        url_args = [a for a in cmd if a.startswith("https://www.instagram.com/")]
        entry = {
            "args": [a if a not in url_args else "<url>" for a in cmd[1:]],
            "url": ident.text(url_args[0]) if url_args else None,
            "exit_code": run["exit"],
            "stdout": None,
            "stderr": None,
            "cookie_changes": run["cookie_changes"],
        }
        if kind == "gallery-dl.dump-json":
            if (kind, "stdout") in documents:
                entry["stdout"] = put(f"{kind}.stdout.json", dumps(ident.scrub_document(documents[(kind, "stdout")])))
        elif kind == "yt-dlp.dump-json":
            entry["stdout_json_lines"] = sum(1 for line in run["stdout"].splitlines() if line.startswith("{"))
        elif run["stdout"].strip():
            entry["stdout"] = put(f"{kind}.stdout.txt", ident.scrub_text(run["stdout"], run["out"]))
        if run["stderr"].strip():
            entry["stderr"] = put(f"{kind}.stderr.txt", ident.scrub_text(run["stderr"], run["out"]))
        if kind.endswith(".download"):
            writes = []
            for name in sorted(os.listdir(run["out"])) if os.path.isdir(run["out"]) else []:
                new_name = ident.text(name)
                item = {"name": new_name, "kind": kind_of(name)}
                if (kind, name) in documents:
                    item["source"] = put(f"files/{new_name}", dumps(ident.scrub_document(documents[(kind, name)])))
                writes.append(item)
            entry["writes"] = writes
        case_runs[kind] = entry

    case = {
        "case": args.case,
        "note": args.note,
        "tools": dict(v.split("=", 1) for v in args.tool_version),
        "input_url": next((e["url"] for e in case_runs.values() if e["url"]), None),
        "runs": case_runs,
    }
    put("case.json", dumps(case))

    dump = files.get("gallery-dl.dump-json.stdout.json")
    if dump and "files/info.json" in files:
        post_entry = json.loads(dump)[0][1]
        post_level = json.loads(files["files/info.json"])
        differing = sorted(k for k in set(post_entry) | set(post_level) if post_entry.get(k) != post_level.get(k))
        print(f"{args.case}: dump-json post entry vs info.json, differing keys after scrub: {differing or 'none'}")

    leaks = [(name, leak) for name, body in files.items() for leak in ident.leaks(body)]
    if leaks:
        sys.exit(f"refusing to write, {len(leaks)} real identifiers survived in: {sorted({n for n, _ in leaks})}")

    for name, body in files.items():
        path = Path(args.out) / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(body, encoding="utf-8")
    print(f"{args.case}: wrote {len(files)} files, {len(ident.codes)} codes, {len(ident.handles)} handles")
    print(f"{args.case}: strings passed through unchanged, review: {sorted(ident.unchanged)}")


if __name__ == "__main__":
    main()
