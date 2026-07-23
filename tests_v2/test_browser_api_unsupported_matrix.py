#!/usr/bin/env python3
"""Browser parity matrix: advertised methods + explicit WKWebView not_supported gaps."""

import os
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from cmux import cmux, cmuxError


SOCKET_PATH = os.environ.get("CMUX_SOCKET_PATH", "/tmp/cmux-debug.sock")

# Methods expected to be present in system.capabilities for the browser v2 surface.
EXPECTED_BROWSER_METHODS = {
    "browser.open_split",
    "browser.navigate",
    "browser.back",
    "browser.forward",
    "browser.reload",
    "browser.url.get",
    "browser.focus_webview",
    "browser.is_webview_focused",
    "browser.snapshot",
    "browser.eval",
    "browser.wait",
    "browser.click",
    "browser.dblclick",
    "browser.hover",
    "browser.focus",
    "browser.bringtofront",
    "browser.type",
    "browser.fill",
    "browser.content",
    "browser.innertext",
    "browser.setcontent",
    "browser.setvalue",
    "browser.inserttext",
    "browser.selectall",
    "browser.multiselect",
    "browser.clear",
    "browser.clipboard",
    "browser.keyboard",
    "browser.pause",
    "browser.press",
    "browser.keydown",
    "browser.keyup",
    "browser.check",
    "browser.uncheck",
    "browser.select",
    "browser.scroll",
    "browser.scroll_into_view",
    "browser.screenshot",
    "browser.get.text",
    "browser.get.html",
    "browser.get.value",
    "browser.get.attr",
    "browser.get.title",
    "browser.get.count",
    "browser.get.box",
    "browser.get.styles",
    "browser.is.visible",
    "browser.is.enabled",
    "browser.is.checked",
    "browser.find.role",
    "browser.find.text",
    "browser.find.label",
    "browser.find.placeholder",
    "browser.find.alt",
    "browser.find.title",
    "browser.find.testid",
    "browser.find.first",
    "browser.find.last",
    "browser.find.nth",
    "browser.frame.select",
    "browser.frame.main",
    "browser.dialog.accept",
    "browser.dialog.dismiss",
    "browser.download.wait",
    "browser.cookies.get",
    "browser.cookies.set",
    "browser.cookies.clear",
    "browser.storage.get",
    "browser.storage.set",
    "browser.storage.clear",
    "browser.tab.new",
    "browser.tab.list",
    "browser.tab.switch",
    "browser.tab.close",
    "browser.console.list",
    "browser.console.clear",
    "browser.errors.list",
    "browser.highlight",
    "browser.state.save",
    "browser.state.load",
    "browser.addinitscript",
    "browser.addscript",
    "browser.addstyle",
    "browser.dispatch",
    "browser.evalhandle",
    "browser.expose",
    "browser.viewport.set",
    "browser.geolocation.set",
    "browser.offline.set",
    "browser.headers.set",
    "browser.credentials.set",
    "browser.permissions.set",
    "browser.permissions",
    "browser.useragent.set",
    "browser.useragent",
    "browser.locale.set",
    "browser.locale",
    "browser.timezone.set",
    "browser.timezone",
    "browser.media.set",
    "browser.device.set",
    "browser.trace.start",
    "browser.trace.stop",
    "browser.har.start",
    "browser.har.stop",
    "browser.har_start",
    "browser.har_stop",
    "browser.network.route",
    "browser.network.unroute",
    "browser.network.requests",
    "browser.network.responsebody",
    "browser.responsebody",
    "browser.screencast.start",
    "browser.screencast.stop",
    "browser.screencast_start",
    "browser.screencast_stop",
    "browser.video_start",
    "browser.video_stop",
    "browser.input_mouse",
    "browser.input_keyboard",
    "browser.input_touch",
}

# Commands that are intentionally exposed but must return not_supported on WKWebView.
WKWEBVIEW_NOT_SUPPORTED = {
    "browser.geolocation.set": {"latitude": 37.7749, "longitude": -122.4194},
    "browser.offline.set": {"enabled": True},
    "browser.trace.start": {},
    "browser.trace.stop": {},
    "browser.network.route": {"url": "**/*"},
    "browser.network.unroute": {"url": "**/*"},
    "browser.network.requests": {},
    "browser.screencast.start": {},
    "browser.screencast.stop": {},
    "browser.input_mouse": {"args": ["move", "10", "10"]},
    "browser.input_keyboard": {"args": ["type", "hello"]},
    "browser.input_touch": {"args": ["tap", "10", "10"]},
} if sys.platform == "darwin" else {}


def _must(cond: bool, msg: str) -> None:
    if not cond:
        raise cmuxError(msg)


def _expect_not_supported(c: cmux, method: str, params: dict) -> None:
    try:
        c._call(method, params)
    except cmuxError as exc:
        text = str(exc)
        if "not_supported" in text:
            return
        raise cmuxError(f"Expected not_supported for {method}, got: {text}")
    raise cmuxError(f"Expected not_supported for {method}, but call succeeded")


def main() -> int:
    with cmux(SOCKET_PATH) as c:
        caps = c.capabilities() or {}
        methods = set(caps.get("methods") or [])

        missing = sorted(EXPECTED_BROWSER_METHODS - methods)
        _must(not missing, f"Missing expected browser methods in capabilities: {missing}")

        opened = c._call("browser.open_split", {"url": "about:blank"}) or {}
        sid = str(opened.get("surface_id") or "")
        _must(bool(sid), f"browser.open_split returned no surface_id: {opened}")
        front = c._call("browser.bringtofront", {"surface_id": sid}) or {}
        _must(bool(front.get("focused")) is True, f"browser.bringtofront failed: {front}")

        viewport = c._call("browser.viewport.set", {"surface_id": sid, "width": 320, "height": 240}) or {}
        viewport_value = viewport.get("viewport") or {}
        _must(int(viewport_value.get("width") or 0) == 320, f"browser.viewport.set width failed: {viewport}")
        _must(int(viewport_value.get("height") or 0) == 240, f"browser.viewport.set height failed: {viewport}")

        if sys.platform == "darwin":
            for method, extra in WKWEBVIEW_NOT_SUPPORTED.items():
                payload = {"surface_id": sid}
                payload.update(extra)
                _expect_not_supported(c, method, payload)
            print("PASS: browser method matrix is explicit (capabilities + WKWebView not_supported contract)")
            return 0

        geo = c._call("browser.geolocation.set", {"surface_id": sid, "latitude": 37.7749, "longitude": -122.4194, "accuracy": 9}) or {}
        geo_value = geo.get("geolocation") or {}
        _must(abs(float(geo_value.get("latitude") or 0) - 37.7749) < 0.0001, f"browser.geolocation.set latitude failed: {geo}")
        _must(abs(float(geo_value.get("longitude") or 0) + 122.4194) < 0.0001, f"browser.geolocation.set longitude failed: {geo}")
        offline = c._call("browser.offline.set", {"surface_id": sid, "enabled": True}) or {}
        _must(bool(offline.get("offline")) is True, f"browser.offline.set true failed: {offline}")
        online_probe = c._call("browser.eval", {"surface_id": sid, "script": "navigator.onLine"}) or {}
        _must(bool(online_probe.get("value")) is False, f"Expected navigator.onLine=false while offline: {online_probe}")
        c._call("browser.offline.set", {"surface_id": sid, "enabled": False})

        ua = c._call("browser.useragent.set", {"surface_id": sid, "user_agent": "cmux-matrix-agent/1.0"}) or {}
        _must(str(ua.get("user_agent") or "") == "cmux-matrix-agent/1.0", f"browser.useragent.set failed: {ua}")
        locale = c._call("browser.locale.set", {"surface_id": sid, "locale": "fr-FR"}) or {}
        _must(str(locale.get("locale") or "") == "fr-FR", f"browser.locale.set failed: {locale}")
        timezone = c._call("browser.timezone.set", {"surface_id": sid, "timezone": "Europe/Paris"}) or {}
        _must(str(timezone.get("timezone") or "") == "Europe/Paris", f"browser.timezone.set failed: {timezone}")
        media = c._call(
            "browser.media.set",
            {
                "surface_id": sid,
                "media_type": "print",
                "color_scheme": "dark",
                "reduced_motion": "reduce",
            },
        ) or {}
        media_value = media.get("media") or {}
        _must(str(media_value.get("media_type") or "") == "print", f"browser.media.set type failed: {media}")
        _must(str(media_value.get("color_scheme") or "") == "dark", f"browser.media.set color scheme failed: {media}")
        device = c._call(
            "browser.device.set",
            {"surface_id": sid, "device": "iphone-13", "user_agent": "cmux-matrix-agent/1.0"},
        ) or {}
        device_value = device.get("device") or {}
        _must(bool(device_value.get("mobile")) is True and bool(device_value.get("touch")) is True, f"browser.device.set failed: {device}")
        headers = c._call("browser.headers.set", {"surface_id": sid, "headers": {"X-Cmux-Matrix": "enabled"}}) or {}
        _must(str((headers.get("headers") or {}).get("X-Cmux-Matrix") or "") == "enabled", f"browser.headers.set failed: {headers}")
        credentials = c._call("browser.credentials.set", {"surface_id": sid, "username": "matrix", "password": "secret"}) or {}
        _must(bool(credentials.get("authenticated")) is True, f"browser.credentials.set failed: {credentials}")
        permissions = c._call("browser.permissions.set", {"surface_id": sid, "permission": "clipboard-read", "state": "granted"}) or {}
        _must(str((permissions.get("permissions") or {}).get("clipboard-read") or "") == "granted", f"browser.permissions.set failed: {permissions}")
        ua_probe = c._call("browser.eval", {"surface_id": sid, "script": "navigator.userAgent"}) or {}
        _must(str(ua_probe.get("value") or "") == "cmux-matrix-agent/1.0", f"navigator.userAgent emulation failed: {ua_probe}")
        locale_probe = c._call("browser.eval", {"surface_id": sid, "script": "navigator.language"}) or {}
        _must(str(locale_probe.get("value") or "") == "fr-FR", f"navigator.language emulation failed: {locale_probe}")
        timezone_probe = c._call(
            "browser.eval",
            {"surface_id": sid, "script": "Intl.DateTimeFormat().resolvedOptions().timeZone"},
        ) or {}
        _must(str(timezone_probe.get("value") or "") == "Europe/Paris", f"timezone emulation failed: {timezone_probe}")
        dark_probe = c._call("browser.eval", {"surface_id": sid, "script": "window.matchMedia('(prefers-color-scheme: dark)').matches"}) or {}
        _must(bool(dark_probe.get("value")) is True, f"color scheme emulation failed: {dark_probe}")
        touch_probe = c._call("browser.eval", {"surface_id": sid, "script": "navigator.maxTouchPoints"}) or {}
        _must(int(touch_probe.get("value") or 0) > 0, f"touch emulation failed: {touch_probe}")
        permission_probe = c._call("browser.eval", {"surface_id": sid, "script": "navigator.permissions.query({name: 'clipboard-read'})"}) or {}
        _must(str(((permission_probe.get("value") or {}).get("state")) or "") == "granted", f"permission eval failed: {permission_probe}")
        ua_alias = c._call("browser.useragent", {"surface_id": sid, "value": "cmux-matrix-alias/1.0"}) or {}
        _must(str(ua_alias.get("user_agent") or "") == "cmux-matrix-alias/1.0", f"browser.useragent alias failed: {ua_alias}")
        locale_alias = c._call("browser.locale", {"surface_id": sid, "value": "nl-NL"}) or {}
        _must(str(locale_alias.get("locale") or "") == "nl-NL", f"browser.locale alias failed: {locale_alias}")
        timezone_alias = c._call("browser.timezone", {"surface_id": sid, "value": "Europe/Amsterdam"}) or {}
        _must(str(timezone_alias.get("timezone") or "") == "Europe/Amsterdam", f"browser.timezone alias failed: {timezone_alias}")
        permissions_alias = c._call("browser.permissions", {"surface_id": sid, "permission": "clipboard-write", "state": "granted"}) or {}
        _must(str((permissions_alias.get("permissions") or {}).get("clipboard-write") or "") == "granted", f"browser.permissions alias failed: {permissions_alias}")
        c._call("browser.navigate", {"surface_id": sid, "url": "data:text/html,<input id='name'><button id='btn'>Apply</button><div id='out'>ready</div>"})
        expose = c._call("browser.expose", {"surface_id": sid, "name": "__cmuxMatrixExpose"}) or {}
        _must(bool(expose.get("exposed")) is True, f"browser.expose failed: {expose}")
        expose_probe = c._call("browser.eval", {"surface_id": sid, "script": "typeof window.__cmuxMatrixExpose === 'function'"}) or {}
        _must(bool(expose_probe.get("value")) is True, f"exposed function eval failed: {expose_probe}")
        dispatch = c._call(
            "browser.dispatch",
            {"surface_id": sid, "selector": "#name", "type": "input", "detail": {"value": "matrix-dispatch"}},
        ) or {}
        _must(str(((dispatch.get("event") or {}).get("type")) or "") == "input", f"browser.dispatch failed: {dispatch}")
        handle = c._call("browser.evalhandle", {"surface_id": sid, "script": "document.querySelector('#name').value"}) or {}
        _must(str(handle.get("value") or "") == "matrix-dispatch", f"browser.evalhandle failed: {handle}")

        content = c._call(
            "browser.setcontent",
            {
                "surface_id": sid,
                "html": "<!doctype html><title>matrix-content</title><input id='name'><select id='sel' multiple><option value='a'>A</option><option value='b'>B</option></select><div id='out'>ready</div>",
            },
        ) or {}
        _must(str(content.get("title") or "") == "matrix-content", f"browser.setcontent failed: {content}")
        content_read = c._call("browser.content", {"surface_id": sid}) or {}
        _must("matrix-content" in str(content_read.get("html") or ""), f"browser.content failed: {content_read}")
        inner = c._call("browser.innertext", {"surface_id": sid, "selector": "#out"}) or {}
        _must(str(inner.get("value") or "") == "ready", f"browser.innertext failed: {inner}")
        set_value = c._call("browser.setvalue", {"surface_id": sid, "selector": "#name", "value": "matrix"}) or {}
        _must(str(set_value.get("value") or "") == "matrix", f"browser.setvalue failed: {set_value}")
        inserted = c._call("browser.inserttext", {"surface_id": sid, "selector": "#name", "text": "-edit"}) or {}
        _must(str(inserted.get("value") or "") == "matrix-edit", f"browser.inserttext failed: {inserted}")
        multi = c._call("browser.multiselect", {"surface_id": sid, "selector": "#sel", "values": ["a", "b"]}) or {}
        _must(str(multi.get("value") or "") == "a,b", f"browser.multiselect failed: {multi}")
        selected = c._call("browser.selectall", {"surface_id": sid, "selector": "#name"}) or {}
        _must(str(selected.get("text") or "") == "matrix-edit", f"browser.selectall failed: {selected}")
        clipboard = c._call("browser.clipboard", {"surface_id": sid, "action": "copy"}) or {}
        _must(str(clipboard.get("text") or "") == "matrix-edit", f"browser.clipboard copy failed: {clipboard}")
        clipboard_probe = c._call("browser.eval", {"surface_id": sid, "script": "navigator.clipboard.readText()"}) or {}
        _must(str(clipboard_probe.get("value") or "") == "matrix-edit", f"clipboard eval failed: {clipboard_probe}")
        c._call("browser.clear", {"surface_id": sid, "selector": "#name"})
        cleared_value = c._call("browser.get.value", {"surface_id": sid, "selector": "#name"}) or {}
        _must(str(cleared_value.get("value") or "") == "", f"browser.clear failed: {cleared_value}")
        c._call("browser.focus", {"surface_id": sid, "selector": "#name"})
        keyboard = c._call("browser.keyboard", {"surface_id": sid, "args": ["type", "kbd"]}) or {}
        _must(str((keyboard.get("event") or {}).get("device") or "") == "keyboard", f"browser.keyboard failed: {keyboard}")
        keyboard_value = c._call("browser.get.value", {"surface_id": sid, "selector": "#name"}) or {}
        _must(str(keyboard_value.get("value") or "") == "kbd", f"browser.keyboard did not type into focused element: {keyboard_value}")
        pause = c._call("browser.pause", {"surface_id": sid, "duration_ms": 1}) or {}
        _must(bool(pause.get("paused")) is True, f"browser.pause failed: {pause}")

        trace = c._call("browser.trace.start", {"surface_id": sid}) or {}
        _must(bool(trace.get("tracing")) is True, f"browser.trace.start failed: {trace}")
        har_alias = c._call("browser.har_start", {"surface_id": sid}) or {}
        _must(bool(har_alias.get("har")) is True, f"browser.har_start failed: {har_alias}")
        har_alias_stop = c._call("browser.har_stop", {"surface_id": sid}) or {}
        _must(bool(har_alias_stop.get("har")) is False, f"browser.har_stop failed: {har_alias_stop}")
        har = c._call("browser.har.start", {"surface_id": sid}) or {}
        _must(bool(har.get("har")) is True, f"browser.har.start failed: {har}")
        cast = c._call("browser.screencast.start", {"surface_id": sid}) or {}
        _must(bool(cast.get("screencast")) is True and int(cast.get("frame_count") or 0) >= 1, f"browser.screencast.start failed: {cast}")
        key = c._call("browser.input_keyboard", {"surface_id": sid, "args": ["type", "matrix"]}) or {}
        _must(str((key.get("event") or {}).get("device") or "") == "keyboard", f"browser.input_keyboard failed: {key}")
        mouse = c._call("browser.input_mouse", {"surface_id": sid, "args": ["move", "10", "10"]}) or {}
        _must(str((mouse.get("event") or {}).get("device") or "") == "mouse", f"browser.input_mouse failed: {mouse}")
        touch = c._call("browser.input_touch", {"surface_id": sid, "args": ["tap", "10", "10"]}) or {}
        _must(str((touch.get("event") or {}).get("device") or "") == "touch", f"browser.input_touch failed: {touch}")
        cast_stop = c._call("browser.screencast.stop", {"surface_id": sid}) or {}
        _must(bool(cast_stop.get("screencast")) is False and int(cast_stop.get("frame_count") or 0) >= 1, f"browser.screencast.stop failed: {cast_stop}")
        cast_alias = c._call("browser.screencast_start", {"surface_id": sid}) or {}
        _must(bool(cast_alias.get("screencast")) is True, f"browser.screencast_start failed: {cast_alias}")
        cast_alias_stop = c._call("browser.screencast_stop", {"surface_id": sid}) or {}
        _must(bool(cast_alias_stop.get("screencast")) is False, f"browser.screencast_stop failed: {cast_alias_stop}")
        video = c._call("browser.video_start", {"surface_id": sid}) or {}
        _must(bool(video.get("video")) is True and int(video.get("frame_count") or 0) >= 1, f"browser.video_start failed: {video}")
        video_stop = c._call("browser.video_stop", {"surface_id": sid}) or {}
        _must(bool(video_stop.get("video")) is False and int(video_stop.get("frame_count") or 0) >= 1, f"browser.video_stop failed: {video_stop}")
        trace_stop = c._call("browser.trace.stop", {"surface_id": sid}) or {}
        _must(bool(trace_stop.get("tracing")) is False and int(trace_stop.get("event_count") or 0) >= 1, f"browser.trace.stop failed: {trace_stop}")

        route = c._call(
            "browser.network.route",
            {
                "surface_id": sid,
                "url": "*matrix-network*",
                "body": "<!doctype html><title>matrix-network</title><div id='matrix-network'>ok</div>",
            },
        ) or {}
        route_id = int(route.get("route_id") or 0)
        _must(route_id > 0, f"browser.network.route returned no route id: {route}")
        c._call("browser.navigate", {"surface_id": sid, "url": "data:text/html,matrix-network"})
        har_stop = c._call("browser.har.stop", {"surface_id": sid}) or {}
        _must(bool(har_stop.get("har")) is False and int(har_stop.get("entry_count") or 0) >= 1, f"browser.har.stop failed: {har_stop}")
        har_entries = (((har_stop.get("artifact") or {}).get("log") or {}).get("entries") or [])
        _must(any("matrix-network" in str(((entry.get("response") or {}).get("content") or {}).get("text") or "") for entry in har_entries), f"HAR did not include routed body: {har_stop}")
        network = c._call("browser.network.requests", {"surface_id": sid}) or {}
        requests = network.get("requests") or []
        routed_rows = [row for row in requests if bool(row.get("routed")) and int(row.get("route_id") or 0) == route_id]
        _must(bool(routed_rows), f"Expected routed request: {network}")
        routed_headers = routed_rows[0].get("request_headers") or {}
        _must(str(routed_headers.get("X-Cmux-Matrix") or "") == "enabled", f"browser request headers missing custom header: {network}")
        _must(str(routed_headers.get("Authorization") or "").startswith("Basic "), f"browser request headers missing Authorization: {network}")
        response_body = c._call(
            "browser.network.responsebody",
            {"surface_id": sid, "request_id": routed_rows[0].get("id")},
        ) or {}
        _must("matrix-network" in str(response_body.get("body") or ""), f"browser.network.responsebody failed: {response_body}")
        response_body_alias = c._call(
            "browser.responsebody",
            {"surface_id": sid, "request_id": routed_rows[0].get("id")},
        ) or {}
        _must("matrix-network" in str(response_body_alias.get("body") or ""), f"browser.responsebody failed: {response_body_alias}")
        unroute = c._call("browser.network.unroute", {"surface_id": sid, "url": "*matrix-network*"}) or {}
        _must(int(unroute.get("removed") or 0) == 1, f"browser.network.unroute failed: {unroute}")

        for method, extra in WKWEBVIEW_NOT_SUPPORTED.items():
            payload = {"surface_id": sid}
            payload.update(extra)
            _expect_not_supported(c, method, payload)

    print("PASS: browser method matrix is explicit (capabilities + WKWebView not_supported contract)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
