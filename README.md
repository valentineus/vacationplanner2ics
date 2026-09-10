# vacationplanner2ics

A small, stateless Rust service that turns [Vacationplanner](https://vacationplanner.ru/apidoc)
vacations into an iCalendar subscription for Apple Calendar and other calendar apps.
It only calls `GET /v1/vacations/year/{year}`.

## Run

Requires Docker with Docker Compose.

```sh
docker compose up -d --build app
```

The service listens on `127.0.0.1:8080`. `GET /healthz` returns `ok`.
For public access, put an HTTPS reverse proxy in front of it.

## Subscribe

Add a calendar subscription using this URL, replacing the host and placeholder:

```text
https://calendar.example.com/calendar.ics?token=YOUR_API_TOKEN
```

To select years:

```text
https://calendar.example.com/calendar.ics?token=YOUR_API_TOKEN&years=2026,2027,2028
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

## License

[MIT](LICENSE)
