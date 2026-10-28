# 0001. Instagram downloader

Status: accepted, 2026-10-28. Spike for issue #3. Issue #10 builds on it and issue #8 builds on its cookie findings.

## Decision

Use gallery-dl for every Instagram post. Run it once per import with the command below, and read the post's caption, hashtags and author from the JSON files it writes next to the media. yt-dlp stays in the image for the video platforms planned after v1 and is not called for Instagram.

| Tool | Version | Installed from |
| --- | --- | --- |
| gallery-dl | 1.32.18 | PyPI, latest on 2026-10-28 |
| yt-dlp | 2026.10.26 | PyPI, latest on 2026-10-28 |

Both ran in a virtualenv on Python 3.14.5. ffmpeg 8.1.1 was on the host. Claims marked "by the source" come from reading the tools' code. Everything else was observed in a run against live Instagram with a logged-in session, or offline or against a local server where it says so. A row marked inferred was not observed. The Not observed section lists what no run covered.

## Why gallery-dl

- yt-dlp cannot download a photo. On the photo post it exits 1 with `No video formats found!`. On the six-item carousel it prints JSON for the four videos, reports the same error for the two photos, and exits 1.
- gallery-dl exited 0 on all four live success cases: a photo post, a carousel mixing photos and videos, a reel requested as `/reels/<code>`, and a `/share/reel/<token>` link.
- One gallery-dl run downloads the files, the metadata and each video's cover frame. yt-dlp has nothing for the photos, so it would need a second tool beside it.
- gallery-dl names the cause of a failure. Expired cookies give `HTTP redirect to login page`. yt-dlp gives `Failed to parse JSON`.
- gallery-dl leaves the cookie file alone when extraction fails, as with a login redirect or an HTTP 400. yt-dlp rewrites it after every run, failures included (see Cookies).

The yt-dlp runs used `yt-dlp --cookies <file> -j <url>` and `yt-dlp --cookies <file> --no-progress -o '<dir>/%(id)s.%(ext)s' <url>`.

## Command

```sh
gallery-dl \
	--cookies <import dir>/cookies.txt \
	-D <output dir> \
	-f '{num:>02}.{extension}' \
	--write-info-json --write-metadata \
	-o videos=merged -o previews=video \
	-o sleep-429=0 -o max-posts=1 \
	--filesize-max <MAX_MEDIA_MB>M \
	-- <canonical URL>
```

| Flag | Why |
| --- | --- |
| `--cookies` | The job's copy of the working cookie file. gallery-dl may rewrite it. |
| `-D` | Exact output directory, with no per-user subdirectory. |
| `-f '{num:>02}.{extension}'` | Names files `01.jpg`, `02.mp4`. The default name embeds Instagram media ids. |
| `--write-info-json` | Writes `info.json`, the post-level metadata. |
| `--write-metadata` | Writes `<file>.json` next to every file, with its width, height and URLs. |
| `-o videos=merged` | Downloads Instagram's single MP4 with audio. The default hands each video to yt-dlp in the same process and merges separate streams with ffmpeg. On one 34 s reel that default gave 11.6 MB of VP9 at 1080x1920, and `merged` gave 7.6 MB of H.264 at 720x1280. H.264 with AAC is the most widely playable pair and counts less against `MAX_MEDIA_MB`. Per the source, `merged` also keeps gallery-dl from importing yt-dlp, so the yt-dlp version in `/data/tools` cannot affect it. |
| `-o previews=video` | Also saves each video's cover frame as `NN.jpg`. |
| `-o sleep-429=0` | Fails at once on HTTP 429. The default waits a minute or more before each of four retries, which outlasts `TOOL_TIMEOUT_SECS` (180). |
| `-o max-posts=1` | Stops after one post if a profile URL ever reaches the tool. The tool accepted the option on single posts. A profile URL was not run. |
| `--filesize-max` | Skips any single file over the limit. It limits each file, not the import. |
| `--` | Ends the options, so the URL cannot be read as one. |

The documented command took 0.7 to 1.6 s per post, and the slowest first-time download, the 10-file carousel, took 6.8 s. By the source, a post costs one Instagram API request, two for a `/share/` link, plus one request per file. The tool's default pause between API requests (6 to 12 s) did not come into play, judging by the run times.

To read metadata without downloading, pass `-j` in place of the download flags. The output is a JSON array on stdout. The process exits 0 when extraction fails inside the extractor (see the `-j` note under Failure signatures). The pipeline does not need this mode, because `info.json` carries the same data (the fixtures show the two are identical after scrubbing, and on the raw output of one reel the only differences were signed URLs and profile-picture fields).

## What a run writes

| Case | Files in the output directory | stdout lines |
| --- | --- | --- |
| Photo post | `01.jpg`, `01.jpg.json`, `info.json` | `<dir>/01.jpg` |
| Reel or `/share/` reel | `01.mp4`, `01.mp4.json`, `01.jpg` (cover), `01.jpg.json`, `info.json` | `<dir>/01.mp4`, `<dir>/01.jpg` |
| Photo over `--filesize-max` (`oversize` fixture) | `info.json` only | `# <dir>/01.jpg` |
| Carousel, videos at 1, 3, 4, 5, photos at 2 and 6 | `01.mp4`, `01.jpg`, `02.jpg`, `03.mp4`, `03.jpg`, `04.mp4`, `04.jpg`, `05.mp4`, `05.jpg`, `06.jpg`, each with a `.json`, and `info.json` | one path per media file, in download order |

stdout shows `<path>` for a file the run wrote and `# <path>` for a file it skipped. The `oversize` fixture printed the `# ` form. That run skipped the photo and also wrote no `01.jpg.json`, because by the source the sidecar is written only after a file downloads. Only `info.json` was left. By the source, a file that already exists in the directory is skipped the same way, so a retry into the same directory does not download it again. That retry path was not run.

## Mapping the JSON to a post

`info.json` holds the post-level fields. Each `<NN>.<ext>.json` holds the same fields plus the file's own.

| Post field | Source | Notes |
| --- | --- | --- |
| Caption | `description` | Empty string when the post has no caption. Hashtags and `@mentions` stay inline. |
| Hashtags | `tags` | Absent when the caption has none. Otherwise a list of `#`-prefixed strings, deduplicated and sorted alphabetically, taken from the caption with `#\w+`. |
| Author handle | `username` | No `@`. `fullname` is the display name. `owner_id` is the numeric user id and has no use for the importer. Collaboration posts also carry `coauthors`. |
| Canonical URL | `post_shortcode` | Build `https://www.instagram.com/reel/<code>/` for a post whose only media is one video, and `/p/<code>/` otherwise. Do not use `post_url` or `type`. |
| Media list | The files written | The two-digit prefix is `num`. Kind comes from the extension, `mp4` or `jpg`. `width` and `height` are in the sidecar. Take the duration from ffprobe (see Sidecar sizes and durations). |
| Photo or preview | Sibling file | An `NN.jpg` is a video's cover frame when `NN.mp4` exists (its sidecar has a non-null `video_url` and a `media_id` ending in `p`). Otherwise it is a photo. |
| Cover | `01.jpg` | Present after a complete run. With `previews=video`, item 1 yields an `01.jpg` whether it is a photo or a video. In a photo post it is the photo. In a reel it is the cover frame. |
| Photos | `NN.jpg` without a sibling `NN.mp4` | In a photo-only post they all go to Mealie's `images`. In a mixed post, the ones after the cover become recipe assets. |

### Sidecar sizes and durations

A cover's sidecar (`01.jpg.json` next to an `01.mp4`) holds the video's `width` and `height`, not the JPEG's. A photo's sidecar matches its JPEG. The covers measured in the raw runs:

| Post | Cover JPEG | Cover sidecar |
| --- | --- | --- |
| Reel (`reel` case) | 640x1136 | 720x1280 |
| Carousel, four video covers | 640x798 for item 1, 640x800 for items 3, 4 and 5 | 720x900 |
| `/share/` reel | 1260x2240 | 720x1280 |

Cover size varies between posts, so read it from the file and use the sidecar only for the video.

`audio_duration` is in `info.json` and in both sidecars of the `reel` case, whose post has an original-audio entry. It is 86.4 there, against 86.51 s from ffprobe for the downloaded MP4. The `share` reel has no `audio_duration`, and neither do the photo and the carousel. The field describes the audio track, so #10 takes the duration from ffprobe in every case.

Two fields mislead. `count` includes an audio entry when the post has an original-audio track, and that entry is never downloaded. The maintainer's reel reported `count` 2, `type` `post` and a `/p/<code>/` `post_url`. The `/share/` reel reported `count` 1, `type` `reel` and a `/reel/<code>/` `post_url`. Neither field says whether a post is a reel, and `type` follows `count`. Count the files instead.

## Accepted URL forms

Checked offline by asking gallery-dl which extractor matches, with no request sent.

| Form | Matches |
| --- | --- |
| `/p/<code>`, `/reel/<code>`, `/reels/<code>`, `/tv/<code>` | Yes |
| `/share/<token>`, `/share/p/<token>`, `/share/reel/<token>` | Yes. The tool resolves the redirect itself. Only `/share/reel/` ran live. |
| `/<user>/p/<code>`, `/<user>/reel/<code>` | Yes |
| Any of the above with `?igsh=` or `utm_` parameters | Yes |
| `instagr.am/...` | No. Exit 64, `Unsupported URL`. |
| `instagram.com.evil.test/...` | No. Exit 64. |
| `instagram.com/reels/` with no code | Matches the profile extractor, with `reels` read as a username |

The last row means the platform module must require a code segment before it calls the tool.

## Failure signatures

gallery-dl download command. `<id>` and `<url>` stand for values that vary. The login redirect, the 400 and the oversize rows are recorded in the fixtures (`expired-cookies`, `deleted`, `oversize`). The unsupported-host row comes from offline runs and the 429 row from runs against a local server, so those two have no fixture. The private-post row was not observed.

| Situation | Exit | stderr |
| --- | --- | --- |
| Expired or invalid session | 4 | `[instagram][error] HTTP redirect to login page (https://www.instagram.com/accounts/login/)` |
| Deleted or nonexistent post | 4 | `[instagram][error] HttpError: '400 Bad Request' for 'https://www.instagram.com/api/v1/media/<id>/info/'` |
| Private post (inferred, not observed) | 4 | The 400 row above, or another 4xx `HttpError` from the same endpoint. No private post was available. The maintainer accepted this signature as inferred from the deleted-post path. |
| Unsupported host | 64 | `[gallery-dl][error] Unsupported URL '<url>'` |
| File over `--filesize-max` | 0 | `[downloader.http][warning] File size larger than allowed maximum (<bytes> > <limit>)`, and `# <path>` on stdout |
| Rate limit (local server, not Instagram) | 4 | `[instagram][error] HttpError: '429 Too Many Requests' for 'https://www.instagram.com/api/v1/media/<id>/info/'`, and nothing else with the documented `-o sleep-429=0` |

The 429 row was measured on 2026-10-29 with `backend/tests/fixtures/instagram/scripts/local429.py`. It runs the real gallery-dl command line with every request to `www.instagram.com` redirected to a local server that answers each request with 429. The command was the documented one plus `--config-ignore`, and the URL a made-up shortcode. The local run printed the local address in the error line, and the row shows the address a real run would print. Three settings ran:

| `sleep-429` | Requests the server saw | Wait lines | Exit |
| --- | --- | --- | --- |
| `0`, as in the documented command | 1 | None | 4 |
| `1`, with the default `retries=4` | 5 | Four, 9 to 12 s each, 43.5 s for the run | 4 |
| Not set (the default) | At least 1, stopped after 15 s | `[instagram][info] Waiting for 1 minutes until <time> (429 Too Many Requests)` | Not run to the end |

With `sleep-429=1` the tool made one request and four retries, so five requests, and printed `[instagram][info] Waiting for <n> seconds until <time> (429 Too Many Requests)` before each retry. Each wait was longer than the 1 s setting, because Instagram's own pause between requests (6 to 12 s) sets a floor. With the default the first wait was one minute, and the run was stopped there. By the source, each of the four default waits is about 61 s, which adds up to more than `TOOL_TIMEOUT_SECS`. With `sleep-429=0` the tool prints no wait line. A zero setting turns the 429 wait off, so by the source the response takes the ordinary path for a 4xx status that is not retried, and the error follows the first response. The exit code is the code of the `HttpError` class, which is also what the 400 row exits with.

Notes for #10.

- A run that exits 0 can leave no media. The oversize case does. Treat a run as a success only when it exits 0 and wrote at least one `mp4` or `jpg`.
- Read the status from the `HttpError: '<status> ...'` text. The post-unavailable case was a 400, not a 404.
- The login redirect and the 400 are the only two signatures seen from Instagram. A private post is inferred to land in the second (see Not observed), so word the hint as "private or deleted".
- In `-j` mode the failures that come from the extractor exit 0 and appear as `[[-1, {"error": "<class>", "message": "<text>"}]]` on stdout, with nothing from the tool on stderr. That covers the login redirect (`AbortExtraction`) and the 400 (`HttpError`), seen in 4 live runs (2 each), and the 429 against the local server (`HttpError`). An unsupported URL does not follow that rule. `gallery-dl --config-ignore -j -- https://instagr.am/p/AAAAAAAAAAA/` exits 64, prints `[gallery-dl][error] Unsupported URL 'https://instagr.am/p/AAAAAAAAAAA/'` to stderr and prints nothing to stdout. The oversize row does not apply to `-j`, which downloads nothing.

yt-dlp, for comparison, exits 1 in every failure.

| Situation | stderr |
| --- | --- |
| Photo post | `ERROR: [Instagram] <code>: No video formats found!; please report this issue ...` |
| Expired or invalid session | `ERROR: [Instagram] <code>: Failed to parse JSON (caused by JSONDecodeError("Expecting value in '': line 1 column 1 (char 0)")); please report this issue ...` |
| Deleted or nonexistent post | `ERROR: [Instagram] <code>: Video info extraction failed: HTTP Error 400: Bad Request (caused by <HTTPError 400: Bad Request>)` |

## Cookies

Both tools write the cookie jar back to the file they were given, but under different rules. The table counts the 39 runs with cookie tracking, 36 of them live and 3 offline. Hashes of the file before and after each run decided "changed".

| Run | gallery-dl | yt-dlp |
| --- | --- | --- |
| `-j` that exited 0 | File unchanged, byte for byte (14 runs) | Rewritten (2 runs) |
| Download that wrote files | Rewritten after the files are written (11 runs) | Rewritten (1 run) |
| Run that failed before any file was processed | File unchanged (4 live runs and 3 offline unsupported-URL runs) | Rewritten (4 runs, 2 of them `-j`) |

The 14 gallery-dl `-j` runs include the four whose extraction failed and the two that listed an account's posts, because `-j` exits 0 when the extractor fails. The 11 gallery-dl download runs include the oversize run, which exited 0 with no media and still rewrote the cookies.

In the source, `cookies_store` runs only from gallery-dl's download job, and only once the job has started a post. That matches the table. It also means a download that fails after the post started, such as a CDN error on one file, rewrites the cookies too. That case did not run.

The starting file came from the converter described under Method. It gave every cookie an expiry one year ahead and skipped the empty `csrftoken` and `s_network` values. So "unchanged" below means the tools kept the converter's expiry, and "added" means the converter had left the cookie out.

What changed in the file after a gallery-dl download of a working session, compared by cookie name:

| Cookie | Change |
| --- | --- |
| `sessionid`, `mid`, `datr`, `ig_did`, `dpr` | None. Value and expiry stayed the same in the 10 runs that compared values and in yt-dlp's runs. The first three live runs, the two account listings and the first photo download, compared only length and expiry. |
| `ds_user_id` | Expiry moved to 90 days after the run. The value stayed the same. |
| `csrftoken` | Added by both tools, because the file had none. |
| `rur` | Added, as a session cookie. |
| `ig_pr`, `ig_vw` | Removed by both tools, so the cause is Instagram's response and not either tool (inferred, not traced). |
| `wd` | Removed. gallery-dl clears it on purpose. yt-dlp kept it. |

yt-dlp's `-j` runs and its failed runs also saved the jar. After the invalid-session run it had dropped `sessionid` from the file, which is the bad state the cookie store must never keep.

What this means for #8.

- "A failed run checks nothing in" is the right rule and the rule that matters. With yt-dlp it would be the only protection. With gallery-dl it is needed as well, because a download that fails after the post started also rewrites the file.
- A metadata-only `-j` run needs no check-in.
- The check-in is worth little today. No authentication cookie changed value in 36 live runs of one session (33 compared values, the other three compared length and expiry). Whether `sessionid` rotates over days was not observed.
- gallery-dl writes `<file>.tmp` in the same directory and renames it over the file. The directory must be writable, and the new file gets the process's default mode (0644 observed) in place of the original's 0600. Set the mode again after a check-in.
- The rewritten file has a `# Netscape HTTP Cookie File` header, a blank line, and `0` as the expiry of session cookies. yt-dlp's header has a second comment line. The parser for the working copy must accept comments, blank lines and `0` expiry. The domain stays `.instagram.com` in both.

## Differences from docs/architecture.md

The evidence contradicts or narrows these statements. The architecture document is unchanged.

- The Instagram section says a 404 means the post is private or deleted. A nonexistent post gave a 400 from both tools. No 404 was seen.
- It lists `instagr.am` as an accepted host. gallery-dl does not match it. The canonical URL step must rewrite the host to `www.instagram.com` before the tool runs.
- It says gallery-dl handles every type "with plain HTTP". That holds only with `-o videos=merged`. The default hands videos to yt-dlp and ffmpeg.
- The cookie store section says both tools write refreshed cookies back. gallery-dl does that only after a download run that started a post. yt-dlp does it on every run. Instagram refreshes little of value (see Cookies).
- The Instagram section builds the canonical URL by dropping tracking parameters. That cannot work for a `/share/` link, whose token is not the post's shortcode. The canonical URL takes `post_shortcode` from `info.json`. A reel's `post_url` can also be a `/p/` URL.
- `TOOL_TIMEOUT_SECS` is 180 s, and by the source, gallery-dl's default 429 handling waits at least four minutes in total. Without `sleep-429=0` the runner's kill would hide a rate limit.
- `MAX_MEDIA_MB` caps each download. `--filesize-max` is per file and skips silently with exit 0, so the sum over a carousel is the pipeline's to check.

## Not observed

- A private post. None was available, so the private row in Failure signatures is inferred from the deleted-post path and the fixtures have no `private` case. The maintainer accepted the inference for this spike. A later run needs a `/p/` or `/reel/` URL from a private account that the session's account does not follow.
- A deleted post that once existed. The `deleted` case is a well-formed shortcode that never did. A post deleted later is expected to give the same 400.
- A real rate limit. The 429 row comes from runs against a local server that always answers 429. A throttled session might also surface as a login redirect, which the tool reports the same way as expired cookies. That is a guess, and neither was seen.
- A `/share/` link without a type segment, or `/share/p/`. Only `/share/reel/` ran live.
- Session rotation over days.

## Method

- The recording session was the maintainer's pickled dict of cookie names to values. A loader that refuses every global read it, and it wrote a Netscape file with one line per non-empty cookie in the form `.instagram.com<TAB>TRUE<TAB>/<TAB>TRUE<TAB><expiry one year ahead><TAB><name><TAB><value>`. The file lives outside the repository.
- Each run used its own copy of that file. File hashes and cookie names were compared before and after every run. Values were never printed or stored.
- The photo post is a recipe from a food site. The carousel is a food-trends post from the same site, not a recipe. The reel is a public recipe reel the maintainer supplied. The `/share/` link comes from the gallery-dl test suite and resolves to a public reel that is not a recipe. The invalid-session run used a copy of the cookie file with `sessionid` set to a garbage value.
- The session made 36 live runs. Consecutive runs ended 18 to 184 s apart, and 27 of the 35 gaps were under 40 s. A post run makes one API request (two for a share link) plus one request per file. Two runs listed the recent posts of two food accounts, one run each, to find candidates.
- `backend/tests/fixtures/instagram/` holds the scrubbed fixtures and the scripts that record, scrub and rate-limit-test runs. Its README describes how a stub replays them.
