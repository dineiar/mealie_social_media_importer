# Architecture

Mealie Social Media Importer takes a link to a social media post, downloads the post, transcribes any speech, asks Mealie's own AI to write the recipe, and attaches the post's photo and video to it. v1 supports Instagram only. This file holds the contracts every task builds against. If a PR changes one of them, it updates this file in the same PR.

The approved screen designs are in [`design/`](../design): `login.html`, `login-checking.html`, `login-error.html`, `import.html` and `import-failed.html`.

## Containers

| Container | What it is | Port |
| --- | --- | --- |
| `frontend` | nginx-unprivileged serving the React build and proxying `/api` to the backend | 8080 inside, 9010 on the host in the example compose file |
| `backend` | Rust + axum: the API, the import pipeline, and the external tools gallery-dl, yt-dlp, ffmpeg and ffprobe | 8080, not published |
| `whisper` | A third-party [speaches](https://github.com/speaches-ai/speaches) image: local OpenAI-compatible speech-to-text. In the compose file by default, removable | 8000, not published |
| Mealie | The user's existing Mealie, version 3.29 or later. Not part of this repo | |

The browser never calls Mealie. Mealie sends no CORS headers in production, and every request with an `Authorization` header would need a preflight. Frontend and backend share one origin through the nginx proxy, so the backend sends no CORS headers either.

There is no database. State lives in the browser (the API key), in the backend's memory (recent imports), and on the `/data` volume (files, manifests, cookies).

## Repository layout

```
backend/
  src/
    main.rs          startup, logging, graceful shutdown, `healthcheck` subcommand
    lib.rs           AppState and `router()`, exported for the integration tests
    config.rs        environment parsing and validation
    error.rs         AppError and its JSON response
    auth.rs          Bearer extractor, cached Mealie user lookup
    routes/          health, config, me, platforms, imports, files
    mealie/          MealieApi trait and HTTP client
    platforms/       Platform trait, registry, cookie store, instagram.rs
    pipeline/        import runner and its steps
    store/           in-memory store, manifests, sweeper, signed URLs
    transcribe/      Transcriber trait, ffmpeg extraction, speech API client
    process.rs       child process runner with timeout and kill
  tests/             integration tests and fixtures
  Dockerfile
frontend/
  src/
    api/             fetch client; types/ is generated from Rust by ts-rs
    auth/            key storage and AuthProvider
    pages/           LoginPage, ImportPage
    components/      Header, UrlForm, Sources, ImportCard, Steps, Downloads
    styles/          tokens.css and component CSS ported from design/
  nginx.conf
  Dockerfile
deploy/
  docker-compose.example.yml
  .env.example
docs/                this file, setup guides, decision records
design/              approved mockups
```

## Login and session

1. The user pastes a Mealie API key. With "Remember me" checked, the frontend stores it in `localStorage`. Unchecked, it goes to `sessionStorage`, which the browser clears when the tab closes.
2. Every API call sends `Authorization: Bearer <key>`. The backend validates the key with Mealie's `GET /api/users/self` and caches the answer for 5 minutes, keyed by the SHA-256 of the key. Any 401 from Mealie evicts the entry.
3. On load, the frontend calls `GET /api/me` with the stored key. A 401 clears the key and shows the login form with "Mealie didn't accept this key." A 502 keeps the key and shows a banner saying Mealie can't be reached.
4. "Log off" removes the key from both storages and clears the query cache.
5. `/?url=<post link>` fills the link field and starts the import once the user is logged in. The parameter survives the login screen.

## Backend API

All endpoints live under `/api`. Every endpoint needs `Authorization: Bearer <Mealie API key>`, except `health`, `config` and signed file downloads. Errors come back with the matching status code and `{"error": {"kind": "...", "message": "..."}}`. The shape holds for errors raised by the router and middleware too: an unknown path returns 404 `not_found`, a wrong method 405 `method_not_allowed`, a body over 64 KiB 413 `payload_too_large`, and a response that doesn't start within 660 s 504 `timeout`. `message` is a fixed English string, never the text of an underlying error.

| Method | Path | Behaviour |
| --- | --- | --- |
| GET | `/api/health` | 200 `{"status":"ok"}`. Used by the Docker healthcheck |
| GET | `/api/config` | `{"mealie_public_url", "retention_minutes", "version"}` |
| GET | `/api/me` | `{"id", "username", "full_name", "group_slug", "ai": {"enabled", "image_enabled"}}`. 401 when Mealie rejects the key, 502 when Mealie can't be reached |
| GET | `/api/platforms` | `[{"id":"instagram", "name":"Instagram", "status":"ready" \| "not_configured", "hint"}]` |
| POST | `/api/imports` | Body `{"url": string, "language"?: string}`. 202 with the new import record. 400 for an unsupported host or a platform that isn't set up. If the same URL is already queued or running for this user, 200 with that record. `?wait=true` holds the response until the import finishes, capped at 10 minutes |
| GET | `/api/imports` | The caller's imports, newest first |
| GET | `/api/imports/{id}` | One import. 404 if it belongs to another user |
| POST | `/api/imports/{id}/retry` | Reruns from the first failed step, reusing downloaded files. Uses the key sent with the retry request |
| GET | `/api/files/{id}/{name}?exp=&sig=` | Streams a file with `Content-Disposition: attachment`. 403 for a bad or expired signature, 404 for an unknown file |

`language` is a BCP 47 tag such as `pt-BR`. The frontend always sends `navigator.language`. When the body has none, the backend uses the first tag of `Accept-Language`. When there is neither, the recipe keeps the post's language. The language is stored on the import, so a retry reuses it.

### Import record

API types are defined once in Rust and exported to `frontend/src/api/types/` with ts-rs. CI fails if the committed types differ from a fresh export.

```json
{
  "id": "0b6c2f4e-8a1d-4c2e-9f4b-2f1d7a9e3c10",
  "url": "https://www.instagram.com/reel/C9xYz12AbCd/",
  "platform": "instagram",
  "language": "pt-BR",
  "status": "running",
  "title": null,
  "steps": [
    { "key": "download", "state": "done", "detail": { "media": "video", "bytes": 19083264, "duration_s": 48 } },
    { "key": "transcribe", "state": "active", "detail": null },
    { "key": "create_recipe", "state": "pending", "detail": null },
    { "key": "attach_media", "state": "pending", "detail": null }
  ],
  "error": null,
  "warnings": [],
  "recipe": null,
  "files": [
    { "kind": "video", "name": "video-1.mp4", "mime": "video/mp4", "bytes": 19083264, "url": "/api/files/0b6c2f4e-8a1d-4c2e-9f4b-2f1d7a9e3c10/video-1.mp4?exp=1792520000&sig=…" }
  ],
  "created_at": "2026-10-20T14:02:11Z",
  "finished_at": null,
  "expires_at": null
}
```

- `status`: `queued`, `running`, `succeeded` or `failed`.
- Step `key`: `download`, `transcribe`, `create_recipe`, `attach_media`, in that order.
- Step `state`: `pending`, `active`, `done`, `skipped` or `failed`. These match the five step styles in the mockups. `transcribe` is `skipped` for photo-only posts and when no speech API is configured.
- `error`: `{"kind", "message"}` when `failed`, otherwise null.
- `warnings`: `[{"kind", "message"}]`, where `kind` is `transcription_failed` or `attach_failed`. A warning never changes `status`.
- `recipe`: `{"slug", "name", "url"}` when `succeeded`. `url` is `{MEALIE_PUBLIC_URL}/g/{group_slug}/r/{slug}`.
- `files[].kind`: `video`, `image` or `text`. `caption.txt` and `transcript.txt` are `text`.
- The frontend owns all user-facing wording. The backend sends keys and structured details only.

| `error.kind` | Card title in the UI | Typical cause |
| --- | --- | --- |
| `no_recipe` | No recipe found | Mealie's AI found no ingredients or steps |
| `mealie_unreachable` | Mealie didn't respond | Timeout, DNS failure, refused connection or 5xx |
| `mealie_rejected` | Mealie refused the import | Key revoked (401/403), or Mealie's AI is switched off |
| `download_failed` | Couldn't download the post | Private or deleted post, expired cookies, rate limit |
| `interrupted` | Import interrupted | The backend restarted during the import |

## Import pipeline

```
paste link ─▶ 1 download ─▶ 2 transcribe ─▶ 3 create recipe ─▶ 4 attach media ─▶ succeeded
                 │              │ fails → warning   │                │ fails → warning
                 └──────────────┴───────────────────┴─▶ failed (files stay downloadable, retry resumes here)
```

Only steps 1 and 3 can fail an import.

Before step 1, `POST /api/imports` checks the host against the platform allowlist and the platform status, then queues the import. A semaphore runs `MAX_CONCURRENT_IMPORTS` imports at a time. Each user can have at most 10 imports queued or running.

### 1. Download

The platform module writes everything into `/data/imports/<id>/`: the media, `caption.txt`, and the cover image. It returns a `DownloadedPost` with caption, author handle, hashtags, canonical URL and a media list (kind, file, size, duration).

### 2. Transcribe

The step runs when the post has a video and `TRANSCRIPTION_BASE_URL` is set.

1. ffmpeg extracts mono MP3 at 32 kbps (about 0.24 MB per minute). ffprobe reads the duration.
2. `POST {TRANSCRIPTION_BASE_URL}/audio/transcriptions` as multipart, with `file`, `model` (`TRANSCRIPTION_MODEL`), `language` (`TRANSCRIPTION_LANGUAGE`, only when set) and `response_format=json`. Bearer `TRANSCRIPTION_API_KEY` when set.
3. The result goes to `transcript.txt`. An empty transcript, as from a music-only reel, is a valid result.

Any error becomes a `transcription_failed` warning, and the pipeline continues with the caption only.

### 3. Create the recipe

1. `GET /api/groups/self`. If `aiProviderSettings.aiEnabled` is false, fail with `mealie_rejected`. `aiProviderSettings.imageProviderEnabled` decides whether photos are sent.
2. `POST /api/recipes/create/ai` as `multipart/form-data`:
   - `content`: a header with platform, author and canonical URL, then the caption, then the transcript (see the example below).
   - `images`: one part per photo, for photo posts only, and only when `imageProviderEnabled` is true.
   - `translateLanguage`: the import's `language`, when there is one.
   - `createNewOrganizers`: `MEALIE_CREATE_TAGS`.
   - Never send `url`. Mealie would then fetch the post itself, without cookies. `orgURL` is set in step 4.
3. A 201 response body is the slug as a JSON string, e.g. `"one-pan-lemon-orzo"`. Parse it as JSON.
4. `GET /api/recipes/{slug}`. If `recipeIngredient` and `recipeInstructions` are both empty, or hold only Mealie's placeholders ("Could not detect ingredients", "Could not detect instructions"), `DELETE /api/recipes/{slug}` and fail with `no_recipe`.

Mealie localizes its error messages, so the backend branches on status codes only. A 400 means `no_recipe`, since AI availability was already checked. A 401 or 403 means `mealie_rejected`. A timeout, refused connection or 5xx means `mealie_unreachable`.

Example `content`:

```
Source: Instagram post by @cookwithmara
Link: https://www.instagram.com/reel/C9xYz12AbCd/

Caption:
One-pan lemon orzo 🍋 Save this for later! …

Transcript:
Okay, so start with two tablespoons of butter in a hot pan…
```

### 4. Attach media

1. `PATCH /api/recipes/{slug}` with `{"orgURL": "<canonical URL>"}`. Take the slug from the response from here on.
2. `PUT /api/recipes/{slug}/image` as multipart with `image` (the file) and `extension` (e.g. `jpg`). The cover is the first photo, or the reel's cover frame.
3. `POST /api/recipes/{slug}/assets` as multipart with `name`, `icon`, `extension` and `file`, for each video and each extra photo. Use `mdi-file-image` for photos and `mdi-file` for videos.
4. `GET` the recipe, set `settings.showAssets` to true, and `PATCH` with the whole `settings` object. A partial `settings` object resets the other flags to their defaults.
5. Never send `assets: []` in a PATCH, because it deletes the attached files.

Any failure in this step becomes an `attach_failed` warning. The import still `succeeded`, because the recipe exists.

### Mealie version

The importer requires Mealie 3.29 or later, the first release that accepts MP4 assets. It never reads Mealie's version and has no version-specific code. Whatever an older Mealie rejects surfaces as an ordinary error or warning.

## Storage and retention

```
/data/
  imports/<uuid>/   media, caption.txt, transcript.txt, manifest.json
  cookies/          instagram.txt (working copy), instagram.source (SHA-256 of the original it was seeded from)
  tools/            virtualenv used by TOOLS_AUTO_UPDATE
```

- `manifest.json` is the import record plus what a restart needs. It is rewritten on every state change and never contains the API key.
- `expires_at` is `finished_at` plus `MEDIA_RETENTION_MINUTES` (default 60). A retry resets it.
- A sweeper runs every 60 s and deletes expired imports, both the directory and the memory entry. It only touches `/data/imports/`.
- On startup the backend reloads the manifests. Unexpired imports reappear, imports that were queued or running become `failed` with `interrupted`, and expired ones are deleted.
- The API key is held in memory only while that user's import runs.
- Imports are scoped to the Mealie user id from `/api/users/self`. Users never see each other's imports.
- File links are signed: `sig` is HMAC-SHA256 over `<id>/<name>/<exp>` with a random secret generated at startup, and `exp` equals the import's `expires_at`. After a restart the old links stop working, and the next poll hands out new ones.

## Cookie store

Instagram refreshes some cookies on its responses, and gallery-dl and yt-dlp write the refreshed cookies back to the file they were given. The cookie store keeps those refreshed cookies across container rebuilds without touching the user's file.

- The original is the user's Netscape `cookies.txt` at `INSTAGRAM_COOKIES_FILE`, mounted read-only.
- The working copy is `/data/cookies/instagram.txt`, with `/data/cookies/instagram.source` holding the SHA-256 of the original it was seeded from.
- Seed: at startup and before each job, if the working copy is missing, or the original's hash differs from `instagram.source`, copy the original over the working copy and record its hash. Replacing the original therefore always wins.
- Check out: each job copies the working copy into its import directory and passes that copy to the tool.
- Check in: after a successful run, the job's copy replaces the working copy (write to a temporary file, then rename) under a per-platform mutex. A failed run checks nothing in.
- Status: no original file means `not_configured`, whatever the working copy holds. Otherwise the platform is `ready` when the working copy parses and has a non-empty `sessionid` for `.instagram.com`.

## Instagram

- The downloader is gallery-dl, confirmed by the spike (T03, recorded in `docs/decisions/`). It handles photos, carousels, reels and `/share/` links with plain HTTP.
- Accepted hosts: `instagram.com`, `www.instagram.com`, `instagr.am`. Accepted paths: `/p/<code>`, `/reel/<code>`, `/reels/<code>`, `/tv/<code>`, `/share/…`.
- The canonical URL drops tracking parameters (`igsh`, `utm_*`) and becomes the recipe's `orgURL`.
- Failure hints for `download_failed`: a 401 or a login redirect suggests the cookies expired, a 404 means the post is private or deleted, and a 429 means Instagram is rate-limiting.
- yt-dlp is in the image for the video platforms planned after v1.

## Configuration

Only `MEALIE_URL` is required. A variable set to an empty value counts as unset. The backend validates everything at startup and exits with one line per invalid variable. The `healthcheck` subcommand reads only `BIND_ADDR`.

| Variable | Default | Purpose |
| --- | --- | --- |
| `MEALIE_URL` | required | Base URL the backend uses to reach Mealie, e.g. `http://mealie:9000` |
| `MEALIE_PUBLIC_URL` | `MEALIE_URL` | Base URL for "Open in Mealie" links |
| `MEALIE_TIMEOUT_SECS` | `330` | Per-request timeout, above Mealie's 300 s AI timeout |
| `MEALIE_CREATE_TAGS` | `true` | Sent as `createNewOrganizers` |
| `INSTAGRAM_COOKIES_FILE` | `/config/cookies/instagram.txt` | The original cookies file |
| `TRANSCRIPTION_BASE_URL` | empty | OpenAI-compatible speech API base URL. Empty turns transcription off. `.env.example` sets `http://whisper:8000/v1` |
| `TRANSCRIPTION_API_KEY` | empty | Bearer key for that API |
| `TRANSCRIPTION_MODEL` | `whisper-1` | Model name. `.env.example` sets the bundled container's model |
| `TRANSCRIPTION_LANGUAGE` | empty | ISO 639-1 hint such as `pt`. Empty lets the model detect it |
| `DATA_DIR` | `/data` | Imports, cookies and tools |
| `MEDIA_RETENTION_MINUTES` | `60` | Minutes files and records stay after an import finishes |
| `MAX_MEDIA_MB` | `250` | Download cap per import |
| `MAX_CONCURRENT_IMPORTS` | `2` | Imports running at once |
| `TOOL_TIMEOUT_SECS` | `180` | Kill timeout per gallery-dl, yt-dlp or ffmpeg run |
| `TOOLS_AUTO_UPDATE` | `true` | At most once per 24 h at startup, upgrade gallery-dl and yt-dlp into `/data/tools`. On failure, fall back to the bundled versions |
| `BIND_ADDR` | `0.0.0.0:8080` | Listen address |
| `RUST_LOG` | `info` | Log filter |
| `LOG_FORMAT` | `pretty` | `pretty` or `json` |

The frontend container has one setting, `BACKEND_URL` (default `http://backend:8080`).

## Security rules

- Only hosts registered by a platform reach a downloader. Test against look-alikes such as `instagram.com.evil.test`.
- External tools run through `tokio::process::Command` with an argument vector, the URL after `--`, never through a shell. Each run has a timeout and `kill_on_drop`, and its captured output is capped.
- The API key is wrapped in `secrecy::SecretString`. It is never logged, never written to disk, and never put in a URL or an error message.
- `MAX_MEDIA_MB` caps each download, and per-user and global limits cap concurrent work.
- Both containers run as non-root users. The backend only writes to `/data` and `/tmp`.
- Cookies, session files and `.env` files are never committed.

## Testing

- Backend tests never touch the network. The pipeline depends on the `Downloader`, `Transcriber` and `MealieApi` traits and is tested with fakes. The Mealie and speech API clients are tested against wiremock. Downloaders are tested with stub executables that replay recorded fixtures. Live tests are marked `#[ignore]` and run by hand.
- Frontend tests use Vitest, React Testing Library and MSW, with one test per designed state.
- CI runs `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, Biome, `tsc --noEmit`, Vitest, `vite build`, the ts-rs drift check, and Docker builds of both images.

## Deployment

- `backend/Dockerfile`: build stage `rust:1-slim` with cargo-chef. Runtime `python:3.13-slim` with ffmpeg, tini, and pinned gallery-dl and yt-dlp. uid 10001, `HEALTHCHECK` through the binary's `healthcheck` subcommand.
- `frontend/Dockerfile`: build on `node:24-alpine`, serve from `nginxinc/nginx-unprivileged:alpine`. `nginx.conf` is a template filled with `BACKEND_URL`. It sets an SPA fallback, a long cache for hashed assets and `no-cache` for `index.html`, a 660 s read timeout on `/api`, and a strict Content-Security-Policy.
- `deploy/docker-compose.example.yml` runs `frontend`, `backend` and `whisper`. A comment above `whisper` says to remove it, along with its volume, when `TRANSCRIPTION_*` points at an external speech API. The backend tolerates a missing speech API, so nothing else changes.
- Images are published to `ghcr.io/dineiar/mealie-social-media-importer-backend` and `-frontend` for amd64 and arm64 on `v*` tags.
