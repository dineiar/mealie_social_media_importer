# Instagram fixtures

Recorded runs of gallery-dl 1.32.18 and yt-dlp 2026.10.26 against real public Instagram posts. The downloader tests replay them through a stub `gallery-dl`. Every handle, name, caption, shortcode, numeric id, post date and CDN URL is synthetic. The JSON keeps the structure of the real output. The decision record is [`docs/decisions/0001-instagram-downloader.md`](../../../../docs/decisions/0001-instagram-downloader.md).

## Cases

| Directory | What was requested |
| --- | --- |
| `photo` | A single-photo post |
| `carousel` | Six items. Videos at positions 1, 3, 4 and 5, photos at 2 and 6 |
| `reel` | A reel requested through the `/reels/<code>` path form, with a caption and five hashtags |
| `share` | A `/share/reel/<token>` link that redirects to a reel |
| `deleted` | A well-formed shortcode that does not exist |
| `expired-cookies` | A public post requested with a cookies file whose session cookie is invalid |

There is no `private` case. It needs a post from a private account that the recording account does not follow, and none was available.

## Files in a case

| File | Content |
| --- | --- |
| `case.json` | Tool versions, the synthetic input URL, and one entry per recorded run (see below) |
| `gallery-dl.dump-json.stdout.json` | stdout of the `-j` run |
| `gallery-dl.download.stdout.txt` | stdout of the download run, one line per file, with `{dir}` where the output directory was |
| `gallery-dl.download.stderr.txt` | stderr of the download run. Absent when stderr was empty |
| `files/` | The JSON files the download run writes: `info.json` and one `<NN>.<ext>.json` per media file |
| `yt-dlp.*` | stderr, and stdout for download runs, of the matching yt-dlp run. Its JSON output is not kept, only the number of lines |

A run entry in `case.json` has `args` (with `<cookies>`, `<dir>` and `<url>` where the real values were), `exit_code`, the names of its stdout and stderr files (`null` when empty), and for download runs `writes`. `writes` lists every file the command leaves in the output directory. An entry with a `source` is a JSON file to copy from this directory. An entry without one is a media file. Media is not committed. The test creates a small file of the right kind (`image` or `video`) under the listed name.

## Replaying a run

A stub for a download run does four things in order. It writes every `writes` entry into the directory that follows `-D`. It prints the stdout file with `{dir}` replaced by that directory. It prints the stderr file to stderr. It exits with `exit_code`.

A `-j` run prints the stdout file and exits with `exit_code`. Note that `-j` exits 0 even when extraction fails. The failure is a `[-1, {...}]` entry in the JSON.

## Refreshing the fixtures

Instagram changes its JSON now and then. To record again, run each command through `scripts/capture.py` with a Netscape `cookies.txt` of a logged-in session, then turn the runs into fixtures with `scripts/scrub.py`. Both scripts hold no identifiers. `scrub.py` reads the real values from the recorded data, replaces them, and refuses to write if any survive. It also prints the string values that passed through unchanged. Read that list after each case, because a field Instagram adds later is not scrubbed until a key rule covers it. Keep the capture directories outside the repository, because they hold a copy of the cookies file and the real output.
