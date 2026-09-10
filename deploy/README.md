# VPS deployment

These files configure the maintainer's Debian 12 server with Podman 4.3,
systemd and nginx. The nginx configuration uses the existing wildcard
certificate at `/etc/letsencrypt/live/popov.link/` and the server's TLS settings.

| Source | Installed path |
| --- | --- |
| `vacationplanner.service` | `/etc/systemd/system/vacationplanner.service` |
| `vacationplanner.conf` | `/etc/nginx/sites-available/vacationplanner.conf` |
| `index.html` | `/var/www/vacationplanner/index.html` |

Enable the nginx site with a symlink in `/etc/nginx/sites-enabled/`. Validate
the unit with `systemd-analyze verify`, run `systemctl daemon-reload`, then
`systemctl enable --now vacationplanner.service`. Run `nginx -t` before
`systemctl reload nginx`. Back up existing configuration before replacing it.

The container listens on `127.0.0.1:8080` using host networking. It runs as
`65532:65532`, with a read-only filesystem, no capabilities or additional
privileges, and limits of 128 MiB without swap, 0.5 CPU and 64 processes.
systemd restarts it after an exit and checks `/healthz` before marking the
start successful. Graceful shutdown allows 65 seconds.

Only `/calendar.ics` is proxied. Other valid GET/HEAD paths serve the same
HTML 4.01 Strict page; unsupported methods return `405`. Calendar limits are
6 requests/minute per IP with a burst of 4, 2 requests/second overall with a
burst of 16, and 4 concurrent requests per IP or 16 overall. Rejections return
`429` and `Retry-After: 60`. Client addresses come from nginx's existing
trusted proxy configuration. Both virtual hosts disable access and error
logging; calendar responses are not cached or buffered to disk.

## Updates and recovery

The existing `podman-auto-update.timer` checks the registry daily. The
`io.containers.autoupdate=registry` label and `PODMAN_SYSTEMD_UNIT` connect
the container to its unit. Podman's default rollback restores the previous
image if restarting the updated service fails, including its HTTP readiness
check. Ordinary restarts use the saved image with `--pull=missing`.

```sh
sudo systemctl status vacationplanner.service podman-auto-update.timer
curl --fail http://127.0.0.1:8080/healthz
sudo podman auto-update --dry-run
```

Before an update, retain a known-good image under a separate local tag and
record its digest. This also protects it from the existing timer's image
pruning. To recover manually, tag that saved image as
`code.popov.link/valentineus/vacationplanner2ics:latest` and restart
`vacationplanner.service`. If the registry image is still faulty, temporarily
remove the container's auto-update label from this unit before restarting;
restore it when a fixed image is available. Restore nginx files from the
backup, validate them and reload nginx if the proxy change must be reverted.

HTML validation uses OpenSP with the W3C HTML 4.01 Strict DTD, HTML Tidy and
Lynx in Docker. Deployment checks use a local mock API to exercise query
forwarding, routes, headers, rate limits and concurrent request limits.
Use fake credentials for these checks and verify logging before making a
real calendar request.
