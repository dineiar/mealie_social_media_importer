# CLAUDE.md

Mealie Social Media Importer is a self-hosted web app that turns a social media post into a [Mealie](https://github.com/mealie-recipes/mealie) recipe. The user logs in with a Mealie API key and pastes a post link. The backend downloads the post, transcribes any speech, has Mealie's own AI write the recipe, and attaches the photo and video to it. v1 supports Instagram only.

Read [`docs/architecture.md`](docs/architecture.md) before starting any task. It holds the API contract, the import record, the pipeline's exact Mealie calls, the cookie store, the configuration and the security rules. The approved screen designs are the HTML files in [`design/`](design).

Work is tracked as GitHub issues #1 to #19, grouped into four milestones. Each issue lists its dependencies and its acceptance criteria.

## Stack

- Backend (`backend/`): Rust (edition 2024), axum, tokio, reqwest with rustls, figment, tracing, thiserror, moka, secrecy, ts-rs. It shells out to gallery-dl, yt-dlp, ffmpeg and ffprobe.
- Frontend (`frontend/`): React 19, strict TypeScript, Vite, TanStack Query, plain CSS ported from `design/`, Biome, Vitest, React Testing Library, MSW.
- Deployment (`deploy/`): two Dockerfiles (backend, frontend) and an example compose file that also runs a local Whisper (speaches) container. Images go to GHCR for amd64 and arm64.

## Decisions already made

Don't reopen these in a PR. Raise them in an issue instead.

- Mealie's own AI writes the recipe, through `POST /api/recipes/create/ai`. The importer has no LLM of its own.
- The importer transcribes speech through any OpenAI-compatible speech API. The compose file ships a local Whisper container by default, and users can swap it for OpenAI, Groq or similar.
- One pass: transcribe whenever the post has speech, then call Mealie once with caption and transcript together.
- Mealie 3.29 or later is required. The code never checks Mealie's version, and anything an older Mealie rejects is handled as an ordinary error.
- No database. Imports live in memory with a manifest per import on disk, and they're deleted `MEDIA_RETENTION_MINUTES` (default 60) after they finish.
- The browser holds the API key: in `localStorage` with "Remember me", otherwise in `sessionStorage`. The backend proxies every Mealie call, because Mealie sends no CORS headers.
- The recipe language is the browser's language, sent with each import and passed to Mealie as `translateLanguage`.
- Instagram is downloaded with gallery-dl and a Netscape `cookies.txt`. Refreshed cookies are kept in `/data/cookies/` and survive container rebuilds. The user's mounted file stays read-only.
- The frontend polls `GET /api/imports` every 1.5 s while anything runs. There is no SSE or WebSocket.
- API types are defined once in Rust and exported to TypeScript with ts-rs.

## Commands

These become available once the skeleton issues (#1 and #2) are merged.

```sh
# backend
cd backend
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo run              # needs MEALIE_URL

# frontend
cd frontend
npm ci
npm run dev            # proxies /api to http://localhost:8080
npm run lint           # Biome
npm run typecheck
npm test
npm run build

# whole stack
docker compose -f deploy/docker-compose.example.yml up --build
```

## Working on an issue

- Use one branch and one PR per issue. Name the branch `<issue number>-<short-slug>`, e.g. `4-mealie-client`, and put `Closes #<n>` in the PR description.
- Stay inside the issue's scope. Note follow-ups in the PR description instead of building them.
- Every acceptance criterion needs a test, or a note in the PR on how you checked it by hand.
- If you change a contract (an endpoint, the import record, an error kind, a setting), update `docs/architecture.md` and regenerate the TypeScript types in the same PR.
- CI must be green before review.

## Rules

- Never log, persist or echo the Mealie API key. Keep it in `secrecy::SecretString` on the backend.
- Never commit cookies, session files, `.env` files or real post data. Test fixtures are scrubbed.
- Tests never touch the network. Use fakes behind the `Downloader`, `Transcriber` and `MealieApi` traits, wiremock for HTTP clients, and stub executables for external tools.
- Run external tools with an argument vector, never through a shell, and with a timeout.
- Branch on Mealie's HTTP status codes, never on its error text, because Mealie localizes messages.
- The frontend owns all user-facing wording. The backend sends keys and structured details.
- Follow the tokens, components and copy in `design/`. When a mockup and `docs/architecture.md` disagree, `docs/architecture.md` wins on behaviour and the mockup wins on looks.
- Don't mention or credit other recipe-importer projects in code, comments or docs.
- Write comments and docs in plain, direct sentences. Comment why something is done, not what the code already says.
