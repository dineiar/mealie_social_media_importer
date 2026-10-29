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
| `oversize` | The `photo` post downloaded with `--filesize-max 100k`, which is smaller than the photo |

There is no `private` case. It needs a post from a private account that the recording account does not follow, and none was available. The decision record infers the private-post signature from the `deleted` case, and the maintainer accepted that.

`oversize` is the case for the `MAX_MEDIA_MB` path. Its download run exits 0 and prints `# {dir}/01.jpg`, so stdout names a file that does not exist. Its `writes` list holds only `info.json`. Unlike a complete run it also skips `01.jpg.json`, because by the source gallery-dl writes a file's sidecar only after the file has downloaded. The byte count in its stderr line is synthetic, like every number of seven digits or more.

## Files in a case

| File | Content |
| --- | --- |
| `case.json` | Tool versions, the synthetic input URL, and one entry per recorded run (see below) |
| `gallery-dl.dump-json.stdout.json` | stdout of the `-j` run |
| `gallery-dl.download.stdout.txt` | stdout of the download run, one line per file, with `{dir}` where the output directory was |
| `gallery-dl.download.stderr.txt` | stderr of the download run. Absent when stderr was empty |
| `files/` | The JSON files the download run writes: `info.json` and one `<NN>.<ext>.json` per media file |
| `yt-dlp.*` | stderr, and stdout for download runs, of the matching yt-dlp run. Its JSON output is not kept, only the number of lines |

A run entry in `case.json` has `args` (with `<cookies>`, `<dir>` and `<url>` where the real values were), `exit_code`, the names of its stdout and stderr files (`null` when empty), `cookie_changes`, and for download runs `writes`. `writes` lists every file the command leaves in the output directory. An entry with a `source` is a JSON file to copy from this directory. An entry without one is a media file. Media is not committed. The test creates a small file of the right kind (`image` or `video`) under the listed name.

`cookie_changes` lists cookie names, never values, in four lists: `added`, `removed`, `expiry_changed` and `value_changed`. It says what the tool did to its `--cookies` file during that run. The starting file came from the recording session's converter, which left out the empty `csrftoken` cookie and gave every cookie an expiry a year ahead. So `added` means the starting file lacked the cookie. A run that left the file byte for byte alone has four empty lists, and so do the gallery-dl runs that failed before any file was processed. A gallery-dl download that started a post shows the same changes in every case. The record's Cookies section explains the rules.

## Choosing the case

Several cases share an input URL. `p/Dsynth00001` is the input of `photo`, `carousel`, `deleted`, `expired-cookies` and `oversize`, so a stub cannot tell them apart by the URL. The test picks the case and tells the stub, for example through an environment variable that names the case directory, and the stub reads `case.json` from there. Inside the case, the stub picks the run whose `args` match the call, comparing the flags that decide the mode (`-j`, `-D`, `--filesize-max`) and not the values that vary (`<cookies>`, `<dir>`, `<url>`).

## Replaying a run

A stub for a download run does these steps in order. It writes every `writes` entry into the directory that follows `-D`. It prints the stdout file with `{dir}` replaced by that directory. It prints the stderr file to stderr. It applies `cookie_changes` to the file after `--cookies`, so the test can check what the pipeline does with a rewritten file: add each `added` name with a made-up value, drop each `removed` name, and move each `expiry_changed` expiry. Then it exits with `exit_code`.

A `-j` run prints the stdout file and exits with `exit_code`. A `-j` run exits 0 when extraction fails inside the extractor, as with the `deleted` and `expired-cookies` cases, and the failure is a `[-1, {...}]` entry in the JSON. An unsupported URL is different. It exits 64 with `Unsupported URL` on stderr and prints nothing, and no case records it.

## Refreshing the fixtures

Instagram changes its JSON now and then. To record again, run each command through `scripts/capture.py` with a Netscape `cookies.txt` of a logged-in session, then turn the runs into fixtures with `scripts/scrub.py`. Both scripts hold no identifiers. `scrub.py` reads the real values from the recorded data, replaces them, and refuses to write if any survive. It also prints the string values that passed through unchanged. Read that list after each case, because a field Instagram adds later is not scrubbed until a key rule covers it. Keep the capture directories outside the repository, because they hold a copy of the cookies file and the real output.

`scripts/local429.py` is the one script that is not part of that loop. It runs the real gallery-dl command line against a local server that answers 429, which is how the record's rate-limit row was measured. It sends no request to Instagram.
