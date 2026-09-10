# vacationplanner2ics

A small, stateless Rust service that turns [Vacationplanner](https://vacationplanner.ru/apidoc)
vacations into an iCalendar subscription for Apple Calendar and other calendar apps.
It only calls `GET /v1/vacations/year/{year}`.

## Ready-made Docker images

Both registries provide the same service for `linux/amd64`. Each platform tests,
builds and publishes its own image:

| Registry | Package | Image |
| --- | --- | --- |
| GitHub Container Registry | [GitHub package](https://github.com/valentineus/vacationplanner2ics/pkgs/container/vacationplanner2ics) | `ghcr.io/valentineus/vacationplanner2ics:latest` |
| Gitea Container Registry | [Gitea package](https://code.popov.link/valentineus/-/packages/container/vacationplanner2ics/latest) | `code.popov.link/valentineus/vacationplanner2ics:latest` |

Available tags:

- `latest`: the latest successful `master` build on that platform.
- `sha-<full-commit-sha>`: the tested commit.
- `v*`: the corresponding Git tag; does not overwrite `latest`.

Pull from GitHub:

```sh
docker pull ghcr.io/valentineus/vacationplanner2ics:latest
```

Or pull from Gitea:

```sh
docker pull code.popov.link/valentineus/vacationplanner2ics:latest
```

## Self-hosted usage

Docker is the only runtime requirement. Start either image, for example:

```sh
docker run -d --name vacationplanner2ics --restart unless-stopped --read-only \
  -p 127.0.0.1:8080:8080 ghcr.io/valentineus/vacationplanner2ics:latest
```

To use Gitea, replace the image name with the Gitea image listed above.
Verify that the service is ready:

```sh
curl --fail http://127.0.0.1:8080/healthz
```

The expected response is `ok`. The example exposes the service on
`127.0.0.1:8080`; for public access, put an HTTPS reverse proxy in front of it.
The image serves HTTP only. Its `scratch` filesystem contains a static musl binary
and CA certificates for outgoing HTTPS requests, with no shell or package manager.
It runs as an unprivileged user.

## Calendar subscription and privacy

**Self-hosting is recommended. Do not blindly trust any hosted instance,
including the maintainer's.** Subscription URLs contain your Vacationplanner
API token. Service operators and hosting, CDN or proxy providers that terminate
HTTPS can read the full request URL, including its GET parameters. HTTPS protects
the connection; it does not hide the token from those operators.

The maintainer's instance is at [vacationplanner.popov.link](https://vacationplanner.popov.link/).
Query-string logging is disabled on the maintainer's server, but that is not a
guarantee of confidentiality across the infrastructure. Use it only if you accept
that trust requirement. Be careful with subscription URLs and prefer a server
you control.

Add a calendar subscription using this URL, replacing the token placeholder.
For self-hosting, also replace the host with your own:

```text
https://vacationplanner.popov.link/calendar.ics?token=YOUR_API_TOKEN
```

To select years:

```text
https://vacationplanner.popov.link/calendar.ics?token=YOUR_API_TOKEN&years=2026,2027,2028
```

| Query parameter | Meaning |
| --- | --- |
| `token` | Required Vacationplanner API token; URL-encode its value. |
| `years` | Optional comma-separated years. Defaults to the current UTC year and the next year. Duplicates are removed; up to 10 distinct years, from 1 to 9998. |

Use **New Calendar Subscription** in macOS Calendar or **Add Subscription Calendar**
in iOS Calendar. The calendar app controls the refresh interval.

Events cover whole days, including the final vacation day. IDs stay stable across
refreshes, and vacations spanning multiple years appear once. Approved, pending
and rejected vacations become confirmed, tentative and cancelled events.
Names, departments and comments are included. A failed year request returns an
HTTP error instead of a partial calendar. Nothing is cached or stored on disk.

**Keep subscription URLs private:** they contain the API token. Use HTTPS and
disable query-string logging in your reverse proxy, CDN and monitoring tools.
The service does not log requests, credentials or upstream data, follows no
upstream redirects and returns `Cache-Control: private, no-store`.

## Configuration

| Environment variable | Default | Meaning |
| --- | --- | --- |
| `API_URL` | `https://api.vacationplanner.ru` | API origin: scheme, host and optional port. HTTP is supported for trusted internal endpoints; use HTTPS otherwise. |
| `UPSTREAM_TIMEOUT_SECONDS` | `15` | Timeout per upstream request, from 1 to 60 seconds. |
| `BIND_ADDR` | `0.0.0.0:8080` | Listen address inside the container. |

Compose passes `API_URL` and `UPSTREAM_TIMEOUT_SECONDS` from the shell or an
untracked `.env` file. No server-side token configuration is needed.
Requests are limited to 60 seconds overall, 10 MiB per upstream response and
16 simultaneous calendar requests. Invalid input returns `400`, missing or
rejected credentials `401`/`403`, upstream failures `502`, overload or upstream
rate limiting `503`, and timeouts `504`.

## Develop and test

Open the project in a Dev Container, or use Docker directly. Stop `app` before
starting `dev`; both use port 8080.

```sh
docker compose stop app
docker compose up -d dev
docker compose exec dev cargo run --locked
```

```sh
docker compose exec dev cargo fmt --check
docker compose exec dev cargo clippy --locked --all-targets -- -D warnings
docker compose --profile test run --build --rm test
```

The four E2E tests run the release binary against a local mock API, with network
access disabled. They cover calendar parsing with an independent iCalendar
library, year selection, input/upstream failures and subscription updates.
Tests use fake credentials only.

## Building from source

Clone the canonical repository and build with Docker Compose. This also works
natively on ARM64:

```sh
git clone https://code.popov.link/valentineus/vacationplanner2ics.git
cd vacationplanner2ics
docker compose up -d --build app
```

## CI and dependency updates

[Gitea Actions](https://code.popov.link/valentineus/vacationplanner2ics/actions)
and [GitHub Actions](https://github.com/valentineus/vacationplanner2ics/actions)
run independent CI workflows for `master` pushes, pull requests, `v*` tags and manual runs: formatting,
Clippy with warnings denied, release compilation, the four E2E tests without
network access, and an HTTP/shutdown check of the actual runtime image.
Only successful `master` builds and `v*` tags publish images. Gitea publishes
only to `code.popov.link`; GitHub publishes only to `ghcr.io`.

Both workflows build natively on AMD64, limit compilation to two jobs and package
the already-tested binary. Gitea reuses the runner's BuildKit/Cargo caches;
GitHub saves Docker build layers in its Actions cache.

Gitea publishing uses the Actions secret `REGISTRY_TOKEN`, belonging to `valentineus`
with `write:package` permission. GitHub uses its automatic `GITHUB_TOKEN` with
`packages: write`; no extra publishing secret is needed. No Vacationplanner
token is used in either CI.

Renovate runs on Gitea daily or manually, using `RENOVATE_TOKEN` for Gitea and
`RENOVATE_GITHUB_TOKEN` for dependency metadata. Non-major updates are grouped;
updates require review and are not merged automatically.

## License

[MIT](LICENSE)

---

Repository locations: [Gitea — canonical source](https://code.popov.link/valentineus/vacationplanner2ics) · [GitHub — secondary mirror](https://github.com/valentineus/vacationplanner2ics).

Changes are pushed to Gitea and automatically mirrored to GitHub.
