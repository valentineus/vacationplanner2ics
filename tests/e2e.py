"""Four HTTP end-to-end scenarios; all credentials and vacation records are fake."""

import copy
import datetime as dt
import http.server
import json
import os
import subprocess
import threading
import time
import unittest
import urllib.error
import urllib.parse
import urllib.request

from icalendar import Calendar

TOKEN = "e2e-only-token+with/reserved=characters"
BASE = "http://127.0.0.1:8080"
WORKER = "Иван, Петров; \\" + "Длинное имя " * 12
COMMENT = "First line\r\nSecond; line, with \\ slash\rBEGIN:VEVENT\r\nSUMMARY:injection"


def vacation(identifier=1, start="2026-12-30", end="2027-01-03", **extra):
    return dict(
        id=identifier,
        workerName=WORKER,
        dateStart=start,
        dateEnd=end,
        departmentName="Engineering",
        comment=COMMENT,
        moderationStatus="approved",
        **extra,
    )


def request(path="/calendar.ics", parameters=None, method="GET"):
    url = BASE + path
    if parameters is not None:
        url += "?" + urllib.parse.urlencode(parameters)
    try:
        response = urllib.request.urlopen(urllib.request.Request(url, method=method), timeout=5)
    except urllib.error.HTTPError as error:
        response = error
    with response:
        return response.status, response.headers, response.read()


class Upstream(http.server.BaseHTTPRequestHandler):
    calls = []
    responses = {}

    def log_message(self, *_):
        pass

    def do_GET(self):
        self.calls.append((self.command, self.path, self.headers.get("Authorization")))
        response = self.responses.get(self.path, [])
        if callable(response):
            response = response(self)
        status, headers, payload = response if isinstance(response, tuple) else (200, {}, response)
        body = payload if isinstance(payload, bytes) else json.dumps(payload).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        for name, value in headers.items():
            self.send_header(name, value)
        self.end_headers()
        try:
            self.wfile.write(body)
        except (BrokenPipeError, ConnectionResetError):
            pass


class CalendarE2E(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.upstream = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Upstream)
        cls.thread = threading.Thread(target=cls.upstream.serve_forever, daemon=True)
        cls.thread.start()
        cls.origin = f"http://127.0.0.1:{cls.upstream.server_port}"

    @classmethod
    def tearDownClass(cls):
        cls.upstream.shutdown()
        cls.upstream.server_close()
        cls.thread.join()

    def setUp(self):
        Upstream.calls.clear()
        Upstream.responses.clear()
        self.process = subprocess.Popen(
            ["vacationplanner2ics"],
            env={**os.environ, "API_URL": self.origin, "UPSTREAM_TIMEOUT_SECONDS": "1"},
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
        )
        self.addCleanup(self.stop_service)
        for _ in range(100):
            if self.process.poll() is not None:
                self.fail("service exited before becoming ready")
            try:
                if request("/healthz")[0] == 200:
                    return
            except urllib.error.URLError:
                pass
            time.sleep(0.02)
        self.fail("service did not become ready")

    def stop_service(self):
        self.process.terminate()
        try:
            output, _ = self.process.communicate(timeout=5)
        except subprocess.TimeoutExpired:
            self.process.kill()
            self.process.communicate()
            self.fail("service did not shut down gracefully")
        self.assertEqual(self.process.returncode, 0)
        self.assertNotIn(TOKEN.encode(), output)
        self.assertNotIn(urllib.parse.quote_plus(TOKEN).encode(), output)
        self.assertNotIn(WORKER.encode(), output)

    def get_calendar(self, **parameters):
        status, headers, body = request(parameters={"token": TOKEN, **parameters})
        self.assertEqual(status, 200)
        self.assertEqual(headers["Content-Type"], "text/calendar; charset=utf-8")
        self.assertIn("no-store", headers["Cache-Control"])
        self.assertIn("vacations.ics", headers["Content-Disposition"])
        self.assertNotIn(TOKEN.encode(), body)
        self.assertTrue(body.endswith(b"END:VCALENDAR\r\n"))
        self.assertNotIn(b"\n", body.replace(b"\r\n", b""))
        for line in body.split(b"\r\n"):
            self.assertLessEqual(len(line), 75)
            line.decode("utf-8")
        calendar = Calendar.from_ical(body)
        self.assertEqual(str(calendar["VERSION"]), "2.0")
        self.assertIn("PRODID", calendar)
        for component in calendar.walk():
            self.assertFalse(component.errors)
        return calendar.walk("VEVENT")

    def test_default_years_and_icalendar_compatibility(self):
        year = dt.datetime.now(dt.UTC).year
        first = vacation(start=f"{year}-12-30", end=f"{year + 1}-01-03")
        pending = vacation(2, "2024-02-29", "2024-02-29")
        pending.update(moderationStatus="on_moderation", comment=None, departmentName=None)
        rejected = vacation(3)
        rejected["moderationStatus"] = "rejected"
        Upstream.responses = {
            f"/v1/vacations/year/{year}": [first, pending, rejected],
            f"/v1/vacations/year/{year + 1}": [first],
        }
        events = self.get_calendar()
        self.assertEqual(len(events), 3)
        self.assertEqual(events[0].decoded("DTSTART"), dt.date(year, 12, 30))
        self.assertEqual(events[0].decoded("DTEND"), dt.date(year + 1, 1, 4))
        self.assertEqual(events[1].decoded("DTEND"), dt.date(2024, 3, 1))
        self.assertEqual(str(events[0]["SUMMARY"]), WORKER + " — Vacation")
        self.assertIn(COMMENT.replace("\r\n", "\n").replace("\r", "\n"), str(events[0]["DESCRIPTION"]))
        self.assertEqual([str(event["STATUS"]) for event in events], ["CONFIRMED", "TENTATIVE", "CANCELLED"])
        self.assertTrue(all(event["DTSTART"].params["VALUE"] == "DATE" for event in events))
        self.assertTrue(all(event["DTSTAMP"].dt.utcoffset() == dt.timedelta() for event in events))
        self.assertEqual(Upstream.calls, [
            ("GET", f"/v1/vacations/year/{value}", "Bearer " + TOKEN) for value in (year, year + 1)
        ])

    def test_explicit_years_and_request_validation(self):
        for years, expected in [("2026", [2026]), ("2030,2026,2028,2027,2029,2026", list(range(2026, 2031)))]:
            with self.subTest(years=years):
                Upstream.calls.clear()
                self.assertEqual(self.get_calendar(years=years), [])
                self.assertEqual([call[1] for call in Upstream.calls], [f"/v1/vacations/year/{year}" for year in expected])
        Upstream.calls.clear()
        cases = [
            ({}, 401), ({"token": ""}, 401), ({"token": "bad\r\nheader"}, 401),
            ({"token": TOKEN, "years": ""}, 400),
            *[({"token": TOKEN, "years": value}, 400) for value in ["2026,nope", "2026,", "0", "9999", "2026-2027"]],
            ({"token": TOKEN, "year": "2026"}, 400),
            ({"token": TOKEN, "url": "https://example.invalid"}, 400),
            ([("token", TOKEN), ("token", TOKEN)], 400),
        ]
        for parameters, expected in cases:
            with self.subTest(parameters=parameters):
                status, headers, body = request(parameters=parameters)
                self.assertEqual(status, expected)
                self.assertIn("no-store", headers["Cache-Control"])
                self.assertNotIn(TOKEN.encode(), body)
        self.assertEqual(request(parameters={"token": TOKEN}, method="POST")[0], 405)
        self.assertEqual(request("/v1/workers")[0], 404)
        self.assertEqual(request("/healthz")[2], b"ok")
        self.assertEqual(Upstream.calls, [])

    def test_upstream_failures_never_return_partial_calendars(self):
        invalid_date = vacation()
        invalid_date["dateEnd"] = "not-a-date"
        reversed_dates = vacation(start="2026-02-10", end="2026-02-01")

        def slow(_):
            time.sleep(1.5)
            return []

        cases = [
            ((401, {}, TOKEN.encode()), 401),
            ((403, {}, TOKEN.encode()), 403),
            ((429, {}, TOKEN.encode()), 503),
            ((500, {}, TOKEN.encode()), 502),
            ((302, {"Location": self.origin + "/should-not-be-called"}, b""), 502),
            ((200, {}, b"not json " + TOKEN.encode()), 502),
            ((200, {}, {"result": "error", "message": TOKEN}), 502),
            ((200, {"Content-Length": str(10 * 1024 * 1024 + 1)}, b""), 502),
            ([invalid_date], 502), ([reversed_dates], 502), ([{"id": 1}], 502),
            (slow, 504),
        ]
        for response, expected in cases:
            with self.subTest(expected=expected, case=cases.index((response, expected))):
                Upstream.calls.clear()
                Upstream.responses = {
                    "/v1/vacations/year/2026": [vacation()],
                    "/v1/vacations/year/2027": response,
                }
                status, headers, body = request(parameters={"token": TOKEN, "years": "2026,2027"})
                self.assertEqual(status, expected)
                self.assertIn("no-store", headers["Cache-Control"])
                self.assertNotIn(b"BEGIN:VCALENDAR", body)
                self.assertNotIn(TOKEN.encode(), body)
                self.assertEqual(len(Upstream.calls), 2)

    def test_subscription_refresh_and_token_isolation(self):
        original = vacation()
        Upstream.responses = {"/v1/vacations/year/2026": [original]}
        first = self.get_calendar(years="2026")[0]
        changed = copy.deepcopy(original)
        changed.update(workerName="New name", dateEnd="2027-01-05", moderationStatus="on_moderation")
        Upstream.responses["/v1/vacations/year/2026"] = [changed]
        updated = self.get_calendar(years="2026")[0]
        self.assertEqual(first["UID"], updated["UID"])
        self.assertNotEqual(first["SUMMARY"], updated["SUMMARY"])
        self.assertEqual(updated.decoded("DTEND"), dt.date(2027, 1, 6))
        self.assertEqual(str(updated["STATUS"]), "TENTATIVE")
        Upstream.responses["/v1/vacations/year/2026"] = (
            lambda handler: [changed] if handler.headers["Authorization"] == "Bearer " + TOKEN else []
        )
        self.assertEqual(self.get_calendar(token="another-fake-token", years="2026"), [])
        self.assertEqual(self.get_calendar(years="2026")[0]["UID"], first["UID"])
        Upstream.responses["/v1/vacations/year/2026"] = []
        self.assertEqual(self.get_calendar(years="2026"), [])


if __name__ == "__main__":
    unittest.main(verbosity=2)
