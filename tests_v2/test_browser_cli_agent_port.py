#!/usr/bin/env python3
"""CLI parity smoke checks for extended browser command families."""

import functools
import glob
import http.server
import json
import os
import socketserver
import subprocess
import sys
import tempfile
import threading
import time
from contextlib import contextmanager
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from cmux import cmuxError


SOCKET_PATH = os.environ.get("CMUX_SOCKET_PATH", "/tmp/cmux-debug.sock")


def _must(cond: bool, msg: str) -> None:
    if not cond:
        raise cmuxError(msg)


def _find_cli_binary() -> str:
    env_cli = os.environ.get("CMUXTERM_CLI")
    if env_cli and os.path.isfile(env_cli) and os.access(env_cli, os.X_OK):
        return env_cli

    fixed = os.path.expanduser("~/Library/Developer/Xcode/DerivedData/cmux-tests-v2/Build/Products/Debug/cmux")
    if os.path.isfile(fixed) and os.access(fixed, os.X_OK):
        return fixed

    candidates = glob.glob(os.path.expanduser("~/Library/Developer/Xcode/DerivedData/**/Build/Products/Debug/cmux"), recursive=True)
    candidates += glob.glob("/tmp/cmux-*/Build/Products/Debug/cmux")
    candidates = [p for p in candidates if os.path.isfile(p) and os.access(p, os.X_OK)]
    if not candidates:
        raise cmuxError("Could not locate cmux CLI binary; set CMUXTERM_CLI")
    candidates.sort(key=lambda p: os.path.getmtime(p), reverse=True)
    return candidates[0]


def _run_cli_json(cli: str, args: list[str], retries: int = 4) -> dict:
    last_merged = ""
    for attempt in range(1, retries + 1):
        proc = subprocess.run(
            [cli, "--socket", SOCKET_PATH, "--json"] + args,
            capture_output=True,
            text=True,
            check=False,
        )
        if proc.returncode == 0:
            try:
                return json.loads(proc.stdout or "{}")
            except Exception as exc:  # noqa: BLE001
                raise cmuxError(f"Invalid CLI JSON output for {' '.join(args)}: {proc.stdout!r} ({exc})")

        merged = f"{proc.stdout}\n{proc.stderr}".strip()
        last_merged = merged
        if "Command timed out" in merged and attempt < retries:
            time.sleep(0.2)
            continue
        raise cmuxError(f"CLI failed ({' '.join(args)}): {merged}")

    raise cmuxError(f"CLI failed ({' '.join(args)}): {last_merged}")


def _run_cli_text(cli: str, args: list[str], retries: int = 3) -> str:
    last_merged = ""
    for attempt in range(1, retries + 1):
        proc = subprocess.run(
            [cli, "--socket", SOCKET_PATH] + args,
            capture_output=True,
            text=True,
            check=False,
        )
        if proc.returncode == 0:
            return (proc.stdout or "").strip()

        merged = f"{proc.stdout}\n{proc.stderr}".strip()
        last_merged = merged
        if "Command timed out" in merged and attempt < retries:
            time.sleep(0.2)
            continue
        raise cmuxError(f"CLI failed ({' '.join(args)}): {merged}")

    raise cmuxError(f"CLI failed ({' '.join(args)}): {last_merged}")


def _run_cli_tail_json(cli: str, args: list[str], retries: int = 3) -> dict:
    last_merged = ""
    for attempt in range(1, retries + 1):
        proc = subprocess.run(
            [cli, "--socket", SOCKET_PATH] + args,
            capture_output=True,
            text=True,
            check=False,
        )
        if proc.returncode == 0:
            try:
                return json.loads(proc.stdout or "{}")
            except Exception as exc:  # noqa: BLE001
                raise cmuxError(f"Invalid CLI JSON output for {' '.join(args)}: {proc.stdout!r} ({exc})")

        merged = f"{proc.stdout}\n{proc.stderr}".strip()
        last_merged = merged
        if "Command timed out" in merged and attempt < retries:
            time.sleep(0.2)
            continue
        raise cmuxError(f"CLI failed ({' '.join(args)}): {merged}")

    raise cmuxError(f"CLI failed ({' '.join(args)}): {last_merged}")


def _run_cli_expect_failure(cli: str, args: list[str], needles: list[str]) -> None:
    proc = subprocess.run(
        [cli, "--socket", SOCKET_PATH, "--json"] + args,
        capture_output=True,
        text=True,
        check=False,
    )
    if proc.returncode == 0:
        raise cmuxError(f"Expected CLI failure for {' '.join(args)}, but it succeeded: {proc.stdout}")
    merged = f"{proc.stdout}\n{proc.stderr}"
    if not any(needle in merged for needle in needles):
        raise cmuxError(f"Expected CLI failure containing one of {needles!r} for {' '.join(args)}, got: {merged}")


@contextmanager
def _local_test_server() -> str:
    with tempfile.TemporaryDirectory(prefix="cmux-browser-cli-") as root:
        root_path = Path(root)
        (root_path / "index.html").write_text(
            """<!doctype html>
<html>
  <body>
    <label for=\"name\">CLI Label</label>
    <input id=\"name\" placeholder=\"cli-place\" title=\"cli-title\" data-testid=\"cli-field\" />
    <button id=\"btn\" role=\"button\">Click</button>
    <ul><li class=\"row\">row-a</li><li class=\"row\">row-b</li></ul>
    <div id=\"style-target\">style</div>
  </body>
</html>
""".strip(),
            encoding="utf-8",
        )

        handler = functools.partial(http.server.SimpleHTTPRequestHandler, directory=str(root_path))

        class _TCP(socketserver.TCPServer):
            allow_reuse_address = True

        with _TCP(("127.0.0.1", 0), handler) as httpd:
            port = int(httpd.server_address[1])
            thread = threading.Thread(target=httpd.serve_forever, daemon=True)
            thread.start()
            try:
                yield f"http://127.0.0.1:{port}/index.html"
            finally:
                httpd.shutdown()
                thread.join(timeout=2)


def main() -> int:
    cli = _find_cli_binary()

    with _local_test_server() as page_url:
        identify = _run_cli_json(cli, ["identify"])
        focused = identify.get("focused") or {}
        workspace = str(
            identify.get("workspace_ref")
            or identify.get("workspace_id")
            or focused.get("workspace_ref")
            or focused.get("workspace_id")
            or ""
        )
        _must(bool(workspace), f"Expected workspace handle from identify: {identify}")
        os.environ["CMUX_WORKSPACE_ID"] = workspace

        opened_tail_json = _run_cli_tail_json(
            cli,
            ["browser", "open", page_url, "--workspace", workspace, "--id-format", "both", "--json"],
        )
        tail_surface = str(opened_tail_json.get("surface_ref") or "")
        _must(tail_surface.startswith("surface:"), f"Expected trailing --json browser open to return surface_ref: {opened_tail_json}")
        _must(bool(opened_tail_json.get("surface_id")), f"Expected trailing --id-format both to preserve surface_id: {opened_tail_json}")
        _must("--json" not in str(opened_tail_json.get("url") or ""), f"Trailing output flags leaked into browser open URL: {opened_tail_json}")
        _run_cli_json(cli, ["browser", tail_surface, "wait", "--load-state", "complete", "--timeout-ms", "15000"])
        tail_url_payload = _run_cli_json(cli, ["browser", tail_surface, "url"])
        _must(str(tail_url_payload.get("url") or "").startswith(page_url), f"Expected trailing --json browser open to navigate: {tail_url_payload}")

        opened = _run_cli_json(cli, ["browser", "open", page_url])
        surface = str(opened.get("surface_ref") or opened.get("surface_id") or "")
        _must(bool(surface), f"browser open returned no surface handle: {opened}")
        _must(surface.startswith("surface:"), f"Expected short surface ref from browser open, got: {opened}")

        _run_cli_json(cli, ["browser", surface, "wait", "--load-state", "complete", "--timeout-ms", "15000"])
        front = _run_cli_json(cli, ["browser", surface, "bringtofront"])
        _must(bool(front.get("focused")) is True, f"Expected bringtofront via CLI: {front}")
        snapshot_text = _run_cli_text(cli, ["browser", surface, "snapshot", "--interactive"])
        _must("ref=e" in snapshot_text, f"Expected snapshot text with refs from CLI: {snapshot_text!r}")

        blank_opened = _run_cli_json(cli, ["browser", "open", "about:blank", "--workspace", workspace])
        blank_surface = str(blank_opened.get("surface_ref") or blank_opened.get("surface_id") or "")
        _must(bool(blank_surface), f"Expected about:blank browser open to return a surface: {blank_opened}")
        blank_snapshot = _run_cli_text(cli, ["browser", blank_surface, "snapshot", "--interactive"])
        _must("about:blank" in blank_snapshot and "get url" in blank_snapshot, f"Expected empty snapshot diagnostics for about:blank: {blank_snapshot!r}")

        opened_routed = _run_cli_json(cli, ["browser", "open", page_url, "--workspace", workspace])
        routed_surface = str(opened_routed.get("surface_ref") or opened_routed.get("surface_id") or "")
        _must(bool(routed_surface), f"browser open --workspace returned no surface handle: {opened_routed}")
        _run_cli_json(cli, ["browser", routed_surface, "wait", "--load-state", "complete", "--timeout-ms", "15000"])
        routed_url_payload = _run_cli_json(cli, ["browser", routed_surface, "url"])
        routed_url = str(routed_url_payload.get("url") or "")
        _must(routed_url.startswith(page_url), f"Expected routed URL to start with page URL, got: {routed_url_payload}")
        _must("--workspace" not in routed_url and "--window" not in routed_url, f"Routing flags leaked into URL: {routed_url_payload}")

        goto_url = f"{page_url}?goto=1"
        goto_payload = _run_cli_json(cli, ["browser", surface, "goto", goto_url, "--snapshot-after"])
        _must(bool(goto_payload.get("post_action_snapshot")), f"Expected goto --snapshot-after to include post_action_snapshot: {goto_payload}")
        goto_url_payload = _run_cli_json(cli, ["browser", surface, "url"])
        current_goto_url = str(goto_url_payload.get("url") or "")
        _must(current_goto_url.startswith(goto_url), f"Expected goto --snapshot-after current URL to match target URL: {goto_url_payload}")
        _must("--snapshot-after" not in current_goto_url, f"Expected goto URL to exclude trailing flag text: {goto_url_payload}")

        find_text = _run_cli_json(cli, ["browser", surface, "find", "text", "row-b"])
        _must(str(find_text.get("element_ref") or "").startswith("@e"), f"Expected element_ref from find text: {find_text}")

        # Exercise frame command routing through expected not_found + main reset.
        _run_cli_expect_failure(cli, ["browser", surface, "frame", "#missing-frame"], ["not_found"])
        _run_cli_json(cli, ["browser", surface, "frame", "main"])

        _run_cli_json(cli, ["browser", surface, "cookies", "set", "cli_cookie", "cookie_val", "--url", "https://example.com"])
        cookies_get = _run_cli_json(cli, ["browser", surface, "cookies", "get", "--name", "cli_cookie"])
        _must(any(str(row.get("name")) == "cli_cookie" for row in (cookies_get.get("cookies") or [])), f"Expected cli_cookie via CLI: {cookies_get}")
        _run_cli_json(cli, ["browser", surface, "cookies", "clear", "--name", "cli_cookie"])

        _run_cli_json(cli, ["browser", surface, "storage", "local", "set", "alpha", "one"])
        storage_get = _run_cli_json(cli, ["browser", surface, "storage", "local", "get", "alpha"])
        _must(str(storage_get.get("value") or "") == "one", f"Expected storage value via CLI: {storage_get}")

        _run_cli_json(cli, ["browser", surface, "fill", "#name", "--text", "weather"])
        cleared = _run_cli_json(cli, ["browser", surface, "fill", "#name", "--text", "", "--snapshot-after"])
        _must(bool(cleared.get("post_action_snapshot")), f"Expected post_action_snapshot from fill --snapshot-after: {cleared}")
        cleared_val = _run_cli_json(cli, ["browser", surface, "get", "value", "#name"])
        _must(str(cleared_val.get("value") or "") == "", f"Expected fill with empty text to clear input: {cleared_val}")

        _run_cli_expect_failure(cli, ["browser", surface, "click", "#does-not-exist"], ["not_found", "snapshot"])
        _run_cli_json(cli, ["browser", surface, "storage", "local", "clear", "--key", "alpha"])

        tabs_before = _run_cli_json(cli, ["browser", surface, "tab", "list"])
        tab_new = _run_cli_json(cli, ["browser", surface, "tab", "new", "about:blank"])
        tab_surface = str(tab_new.get("surface_ref") or tab_new.get("surface_id") or "")
        _must(bool(tab_surface), f"Expected tab surface handle via CLI: {tab_new}")
        tabs_after = _run_cli_json(cli, ["browser", tab_surface, "tab", "list"])
        _must(len(tabs_after.get("tabs") or []) >= len(tabs_before.get("tabs") or []) + 1, "Expected tab count increase via CLI")
        _run_cli_json(cli, ["browser", tab_surface, "tab", "switch", surface])
        _run_cli_json(cli, ["browser", surface, "tab", "close", tab_surface])

        addscript = _run_cli_json(cli, ["browser", surface, "addscript", "1 + 2"])
        _must(int(addscript.get("value") or 0) == 3, f"Expected addscript value=3 via CLI: {addscript}")
        _run_cli_json(cli, ["browser", surface, "addinitscript", "window.__cliInit = \"ok\";"])
        expose = _run_cli_json(cli, ["browser", surface, "expose", "__cmuxCliExpose"])
        _must(bool(expose.get("exposed")) is True, f"Expected expose via CLI: {expose}")
        expose_probe = _run_cli_json(cli, ["browser", surface, "eval", "typeof window.__cmuxCliExpose === 'function'"])
        _must(bool(expose_probe.get("value")) is True, f"Expected exposed function eval via CLI: {expose_probe}")
        dispatch = _run_cli_json(cli, ["browser", surface, "dispatch", "#name", "input", "--value", "cli-dispatch"])
        _must(str((dispatch.get("event") or {}).get("type") or "") == "input", f"Expected dispatch via CLI: {dispatch}")
        eval_handle = _run_cli_json(cli, ["browser", surface, "evalhandle", "document.querySelector('#name').value"])
        _must(str(eval_handle.get("value") or "") == "cli-dispatch", f"Expected evalhandle via CLI: {eval_handle}")

        cli_content_html = "<!doctype html><title>cli-content</title><input id='name'><select id='sel' multiple><option value='a'>A</option><option value='b'>B</option></select><button id='btn'>Click</button><div id='out'>ready</div><div id='style-target'>style</div>"
        content_set = _run_cli_json(cli, ["browser", surface, "setcontent", cli_content_html])
        _must(str(content_set.get("title") or "") == "cli-content", f"Expected setcontent via CLI: {content_set}")
        content_read = _run_cli_json(cli, ["browser", surface, "content"])
        _must("cli-content" in str(content_read.get("html") or ""), f"Expected content via CLI: {content_read}")
        inner_text = _run_cli_json(cli, ["browser", surface, "innertext", "#out"])
        _must(str(inner_text.get("value") or "") == "ready", f"Expected innertext via CLI: {inner_text}")
        set_value = _run_cli_json(cli, ["browser", surface, "setvalue", "#name", "cli"])
        _must(str(set_value.get("value") or "") == "cli", f"Expected setvalue via CLI: {set_value}")
        inserted_text = _run_cli_json(cli, ["browser", surface, "inserttext", "#name", "-edit"])
        _must(str(inserted_text.get("value") or "") == "cli-edit", f"Expected inserttext via CLI: {inserted_text}")
        multi = _run_cli_json(cli, ["browser", surface, "multiselect", "#sel", "a", "b"])
        _must(str(multi.get("value") or "") == "a,b", f"Expected multiselect via CLI: {multi}")
        selected_text = _run_cli_json(cli, ["browser", surface, "selectall", "#name"])
        _must(str(selected_text.get("text") or "") == "cli-edit", f"Expected selectall via CLI: {selected_text}")
        clipboard_copy = _run_cli_json(cli, ["browser", surface, "clipboard", "copy"])
        _must(str(clipboard_copy.get("text") or "") == "cli-edit", f"Expected clipboard copy via CLI: {clipboard_copy}")
        clipboard_read = _run_cli_json(cli, ["browser", surface, "clipboard"])
        _must(str(clipboard_read.get("text") or "") == "cli-edit", f"Expected clipboard read via CLI: {clipboard_read}")
        cleared_value = _run_cli_json(cli, ["browser", surface, "clear", "#name"])
        _must(bool(cleared_value.get("cleared")) is True, f"Expected clear via CLI: {cleared_value}")
        value_after_clear = _run_cli_json(cli, ["browser", surface, "get", "value", "#name"])
        _must(str(value_after_clear.get("value") or "") == "", f"Expected clear to reset value via CLI: {value_after_clear}")
        keyboard = _run_cli_json(cli, ["browser", surface, "keyboard", "press", "Enter"])
        _must(str((keyboard.get("event") or {}).get("device") or "") == "keyboard", f"Expected keyboard via CLI: {keyboard}")
        pause = _run_cli_json(cli, ["browser", surface, "pause", "1"])
        _must(bool(pause.get("paused")) is True, f"Expected pause via CLI: {pause}")
        video_file = tempfile.NamedTemporaryFile(delete=False, prefix="cmux-cli-video-", suffix=".json").name
        video_start = _run_cli_json(cli, ["browser", surface, "video", "start", video_file])
        _must(bool(video_start.get("video")) is True, f"Expected video start via CLI: {video_start}")
        video_stop = _run_cli_json(cli, ["browser", surface, "video", "stop"])
        _must(bool(video_stop.get("video")) is False and int(video_stop.get("frame_count") or 0) >= 1, f"Expected video stop via CLI: {video_stop}")
        _must(os.path.exists(video_file) and os.path.getsize(video_file) > 0, f"Expected video artifact via CLI: {video_file}")

        _run_cli_json(cli, ["browser", surface, "addstyle", "#style-target { color: rgb(0, 128, 0); }"])
        styles = _run_cli_json(cli, ["browser", surface, "get", "styles", "#style-target", "--property", "color"])
        _must("0, 128, 0" in str(styles.get("value") or ""), f"Expected style color via CLI: {styles}")

        _run_cli_json(cli, ["browser", surface, "console", "list"])
        _run_cli_json(cli, ["browser", surface, "console", "clear"])
        _run_cli_json(cli, ["browser", surface, "errors", "list"])
        _run_cli_json(cli, ["browser", surface, "highlight", "#btn"])

        state_file = tempfile.NamedTemporaryFile(delete=False, prefix="cmux-cli-state-", suffix=".json").name
        saved = _run_cli_json(cli, ["browser", surface, "state", "save", state_file])
        _must(str(saved.get("path") or "") == state_file, f"Expected saved state path via CLI: {saved}")
        _run_cli_json(cli, ["browser", surface, "state", "load", state_file])

        viewport = _run_cli_json(cli, ["browser", surface, "viewport", "800", "600"])
        viewport_value = viewport.get("viewport") or {}
        _must(int(viewport_value.get("width") or 0) == 800, f"Expected viewport width via CLI: {viewport}")
        _must(int(viewport_value.get("height") or 0) == 600, f"Expected viewport height via CLI: {viewport}")
        screenshot = _run_cli_json(cli, ["browser", surface, "screenshot", "--json"])
        _must(int(screenshot.get("width") or 0) == 800, f"Expected screenshot width to follow viewport: {screenshot}")
        _must(int(screenshot.get("height") or 0) == 600, f"Expected screenshot height to follow viewport: {screenshot}")

        geo = _run_cli_json(cli, ["browser", surface, "geo", "52.3676", "4.9041", "--accuracy", "10"])
        geo_value = geo.get("geolocation") or {}
        _must(abs(float(geo_value.get("latitude") or 0) - 52.3676) < 0.0001, f"Expected geolocation latitude via CLI: {geo}")
        _must(abs(float(geo_value.get("longitude") or 0) - 4.9041) < 0.0001, f"Expected geolocation longitude via CLI: {geo}")
        offline = _run_cli_json(cli, ["browser", surface, "offline", "true"])
        _must(bool(offline.get("offline")) is True, f"Expected offline=true via CLI: {offline}")
        online = _run_cli_json(cli, ["browser", surface, "offline", "false"])
        _must(bool(online.get("online")) is True, f"Expected online=true via CLI: {online}")
        ua = _run_cli_json(cli, ["browser", surface, "useragent", "cmux-cli-agent/1.0"])
        _must(str(ua.get("user_agent") or "") == "cmux-cli-agent/1.0", f"Expected user agent via CLI: {ua}")
        locale = _run_cli_json(cli, ["browser", surface, "locale", "de-DE"])
        _must(str(locale.get("locale") or "") == "de-DE", f"Expected locale via CLI: {locale}")
        timezone = _run_cli_json(cli, ["browser", surface, "timezone", "Europe/Berlin"])
        _must(str(timezone.get("timezone") or "") == "Europe/Berlin", f"Expected timezone via CLI: {timezone}")
        media = _run_cli_json(
            cli,
            ["browser", surface, "media", "print", "--color-scheme", "dark", "--reduced-motion", "reduce"],
        )
        media_value = media.get("media") or {}
        _must(str(media_value.get("media_type") or "") == "print", f"Expected media type via CLI: {media}")
        _must(str(media_value.get("color_scheme") or "") == "dark", f"Expected color scheme via CLI: {media}")
        device = _run_cli_json(
            cli,
            ["browser", surface, "device", "pixel-5", "--user-agent", "cmux-cli-agent/1.0"],
        )
        device_value = device.get("device") or {}
        _must(bool(device_value.get("mobile")) is True, f"Expected mobile device via CLI: {device}")
        _must(bool(device_value.get("touch")) is True, f"Expected touch device via CLI: {device}")
        headers = _run_cli_json(cli, ["browser", surface, "headers", "X-Cmux-CLI=enabled"])
        _must(str((headers.get("headers") or {}).get("X-Cmux-CLI") or "") == "enabled", f"Expected headers via CLI: {headers}")
        credentials = _run_cli_json(cli, ["browser", surface, "credentials", "cli-user", "secret"])
        _must(bool(credentials.get("authenticated")) is True, f"Expected credentials via CLI: {credentials}")
        permissions = _run_cli_json(cli, ["browser", surface, "permissions", "clipboard-read", "granted"])
        _must(str((permissions.get("permissions") or {}).get("clipboard-read") or "") == "granted", f"Expected permissions via CLI: {permissions}")
        ua_probe = _run_cli_json(cli, ["browser", surface, "eval", "navigator.userAgent"])
        _must(str(ua_probe.get("value") or "") == "cmux-cli-agent/1.0", f"Expected user agent eval via CLI: {ua_probe}")
        locale_probe = _run_cli_json(cli, ["browser", surface, "eval", "navigator.language"])
        _must(str(locale_probe.get("value") or "") == "de-DE", f"Expected locale eval via CLI: {locale_probe}")
        timezone_probe = _run_cli_json(
            cli,
            ["browser", surface, "eval", "Intl.DateTimeFormat().resolvedOptions().timeZone"],
        )
        _must(
            str(timezone_probe.get("value") or "") == "Europe/Berlin",
            f"Expected timezone eval via CLI: {timezone_probe}",
        )
        media_probe = _run_cli_json(cli, ["browser", surface, "eval", "window.matchMedia('(prefers-color-scheme: dark)').matches"])
        _must(bool(media_probe.get("value")) is True, f"Expected media eval via CLI: {media_probe}")
        touch_probe = _run_cli_json(cli, ["browser", surface, "eval", "navigator.maxTouchPoints"])
        _must(int(touch_probe.get("value") or 0) > 0, f"Expected touch eval via CLI: {touch_probe}")
        permission_probe = _run_cli_json(cli, ["browser", surface, "eval", "navigator.permissions.query({name: 'clipboard-read'})"])
        _must(str(((permission_probe.get("value") or {}).get("state")) or "") == "granted", f"Expected permission eval via CLI: {permission_probe}")

        trace_file = tempfile.NamedTemporaryFile(delete=False, prefix="cmux-cli-trace-", suffix=".json").name
        trace_start = _run_cli_json(cli, ["browser", surface, "trace", "start", trace_file])
        _must(bool(trace_start.get("tracing")) is True, f"Expected trace start via CLI: {trace_start}")
        cast_start = _run_cli_json(cli, ["browser", surface, "screencast", "start"])
        _must(bool(cast_start.get("screencast")) is True, f"Expected screencast start via CLI: {cast_start}")
        key_event = _run_cli_json(cli, ["browser", surface, "input", "keyboard", "type", "cli-raw"])
        _must(str((key_event.get("event") or {}).get("device") or "") == "keyboard", f"Expected keyboard input event via CLI: {key_event}")
        mouse_event = _run_cli_json(cli, ["browser", surface, "input", "mouse", "move", "11", "12"])
        _must(str((mouse_event.get("event") or {}).get("device") or "") == "mouse", f"Expected mouse input event via CLI: {mouse_event}")
        touch_event = _run_cli_json(cli, ["browser", surface, "input_touch", "tap", "13", "14"])
        _must(str((touch_event.get("event") or {}).get("device") or "") == "touch", f"Expected touch input event via CLI: {touch_event}")
        cast_stop = _run_cli_json(cli, ["browser", surface, "screencast", "stop"])
        _must(bool(cast_stop.get("screencast")) is False and int(cast_stop.get("frame_count") or 0) >= 1, f"Expected screencast stop via CLI: {cast_stop}")
        trace_stop = _run_cli_json(cli, ["browser", surface, "trace", "stop"])
        _must(bool(trace_stop.get("tracing")) is False and int(trace_stop.get("event_count") or 0) >= 1, f"Expected trace stop via CLI: {trace_stop}")
        _must(os.path.exists(trace_file) and os.path.getsize(trace_file) > 0, f"Expected trace file to be written: {trace_file}")

        har_file = tempfile.NamedTemporaryFile(delete=False, prefix="cmux-cli-", suffix=".har").name
        har_start = _run_cli_json(cli, ["browser", surface, "har", "start", har_file])
        _must(bool(har_start.get("har")) is True, f"Expected HAR start via CLI: {har_start}")
        network_route = _run_cli_json(
            cli,
            [
                "browser",
                surface,
                "network",
                "route",
                "*cli-network*",
                "--body",
                "<!doctype html><title>cli-network</title><div id='cli-network'>ok</div>",
            ],
        )
        route_id = int(network_route.get("route_id") or 0)
        _must(route_id > 0, f"Expected route id via CLI network route: {network_route}")
        _run_cli_json(cli, ["browser", surface, "goto", "data:text/html,cli-network"])
        har_stop = _run_cli_json(cli, ["browser", surface, "har", "stop"])
        _must(bool(har_stop.get("har")) is False and int(har_stop.get("entry_count") or 0) >= 1, f"Expected HAR stop via CLI: {har_stop}")
        _must(os.path.exists(har_file) and os.path.getsize(har_file) > 0, f"Expected HAR file to be written: {har_file}")
        har_entries = (((har_stop.get("artifact") or {}).get("log") or {}).get("entries") or [])
        _must(
            any("cli-network" in str(((entry.get("response") or {}).get("content") or {}).get("text") or "") for entry in har_entries),
            f"Expected CLI HAR body capture: {har_stop}",
        )
        network_requests = _run_cli_json(cli, ["browser", surface, "network", "requests"])
        routed_rows = [
            row
            for row in (network_requests.get("requests") or [])
            if bool(row.get("routed")) and int(row.get("route_id") or 0) == route_id
        ]
        _must(
            bool(routed_rows),
            f"Expected routed request via CLI network requests: {network_requests}",
        )
        routed_headers = routed_rows[0].get("request_headers") or {}
        _must(str(routed_headers.get("X-Cmux-CLI") or "") == "enabled", f"Expected CLI custom request header: {network_requests}")
        _must(str(routed_headers.get("Authorization") or "").startswith("Basic "), f"Expected CLI auth request header: {network_requests}")
        network_body = _run_cli_json(
            cli,
            ["browser", surface, "network", "responsebody", str(routed_rows[0].get("id") or "")],
        )
        _must(
            "cli-network" in str(network_body.get("body") or ""),
            f"Expected response body via CLI network responsebody: {network_body}",
        )
        network_unroute = _run_cli_json(cli, ["browser", surface, "network", "unroute", "*cli-network*"])
        _must(int(network_unroute.get("removed") or 0) == 1, f"Expected CLI network unroute to remove route: {network_unroute}")

        legacy_new = _run_cli_text(cli, ["new-pane", "--type", "browser", "--direction", "right", "--url", page_url])
        _must("surface:" in legacy_new, f"Expected new-pane output to prefer short surface refs, got: {legacy_new!r}")

    print("PASS: browser CLI parity commands are wired for extended families")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
