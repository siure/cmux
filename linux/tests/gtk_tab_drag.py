#!/usr/bin/env python3
"""Exercise native tab gestures, including GTK's button/drag gesture arbitration.

Run scripts/test-gtk-tab-drag.sh with an existing GTK-enabled cmux binary.
Only X11/XTest and Python's standard library are required. Pillow is optional
when --screenshot is supplied for failure diagnostics.
"""

import argparse
import ctypes as C
import json
import os
from pathlib import Path
import signal
import socket
import subprocess
import tempfile
import time


def eventually(description, predicate, timeout=5):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        result = predicate()
        if result:
            return result
        time.sleep(0.05)
    raise AssertionError(description)


class X11:
    def __init__(self):
        self.x = C.CDLL("libX11.so.6")
        self.xt = C.CDLL("libXtst.so.6")
        signatures = {
            "XOpenDisplay": ([C.c_char_p], C.c_void_p),
            "XDefaultRootWindow": ([C.c_void_p], C.c_ulong),
            "XQueryTree": ([C.c_void_p, C.c_ulong, C.POINTER(C.c_ulong),
                            C.POINTER(C.c_ulong), C.POINTER(C.POINTER(C.c_ulong)),
                            C.POINTER(C.c_uint)], C.c_int),
            "XGetGeometry": ([C.c_void_p, C.c_ulong, C.POINTER(C.c_ulong),
                              C.POINTER(C.c_int), C.POINTER(C.c_int),
                              *[C.POINTER(C.c_uint)] * 4], C.c_int),
            "XTranslateCoordinates": ([C.c_void_p, C.c_ulong, C.c_ulong,
                                       C.c_int, C.c_int, C.POINTER(C.c_int),
                                       C.POINTER(C.c_int), C.POINTER(C.c_ulong)], C.c_int),
            "XResizeWindow": ([C.c_void_p, C.c_ulong, C.c_uint, C.c_uint], C.c_int),
            "XSetInputFocus": ([C.c_void_p, C.c_ulong, C.c_int, C.c_ulong], C.c_int),
            "XFlush": ([C.c_void_p], C.c_int),
            "XFree": ([C.c_void_p], C.c_int),
            "XCloseDisplay": ([C.c_void_p], C.c_int),
        }
        for name, (arguments, result) in signatures.items():
            function = getattr(self.x, name)
            function.argtypes, function.restype = arguments, result
        self.xt.XTestFakeMotionEvent.argtypes = [C.c_void_p, C.c_int, C.c_int, C.c_int, C.c_ulong]
        self.xt.XTestFakeButtonEvent.argtypes = [C.c_void_p, C.c_uint, C.c_int, C.c_ulong]
        self.display = self.x.XOpenDisplay(None)
        if not self.display:
            raise RuntimeError("Cannot open DISPLAY; run scripts/test-gtk-tab-drag.sh")
        self.root = self.x.XDefaultRootWindow(self.display)

    def geometry(self, window):
        root, child = C.c_ulong(), C.c_ulong()
        x, y = C.c_int(), C.c_int()
        width, height, border, depth = (C.c_uint() for _ in range(4))
        self.x.XGetGeometry(self.display, window, C.byref(root), C.byref(x), C.byref(y),
                            C.byref(width), C.byref(height), C.byref(border), C.byref(depth))
        self.x.XTranslateCoordinates(self.display, window, self.root, 0, 0,
                                     C.byref(x), C.byref(y), C.byref(child))
        return x.value, y.value, width.value, height.value

    def app_window(self):
        root, parent = C.c_ulong(), C.c_ulong()
        children, count = C.POINTER(C.c_ulong)(), C.c_uint()
        self.x.XQueryTree(self.display, self.root, C.byref(root), C.byref(parent),
                          C.byref(children), C.byref(count))
        try:
            # The wrapper owns a fresh X server with only this app, no window manager.
            return next((w for w in children[:count.value]
                         if self.geometry(w)[2] > 500), None)
        finally:
            if children:
                self.x.XFree(children)

    def move(self, x, y):
        self.xt.XTestFakeMotionEvent(self.display, -1, round(x), round(y), 0)
        self.x.XFlush(self.display)

    def button(self, pressed):
        self.xt.XTestFakeButtonEvent(self.display, 1, pressed, 0)
        self.x.XFlush(self.display)

    def click(self, point):
        self.move(*point)
        self.button(True)
        time.sleep(0.05)
        self.button(False)

    def drag(self, start, end):
        self.move(*start)
        self.button(True)
        try:
            time.sleep(0.12)
            for step in range(1, 31):
                self.move(*(a + (b - a) * step / 30 for a, b in zip(start, end)))
                time.sleep(0.02)
            time.sleep(0.12)
        finally:
            self.button(False)


def run(binary, screenshot):
    with tempfile.TemporaryDirectory(prefix="cmux-tab-drag-") as directory:
        root = Path(directory)
        address = str(root / "cmux.sock")
        env = {key: value for key, value in os.environ.items() if not key.startswith("CMUX_")}
        for key in ("WAYLAND_DISPLAY", "BASH_ENV", "ENV", "PROMPT_COMMAND", "ZDOTDIR"):
            env.pop(key, None)
        for key, child in (("HOME", "home"), ("XDG_CONFIG_HOME", "config"),
                           ("XDG_STATE_HOME", "state"), ("XDG_CACHE_HOME", "cache"),
                           ("XDG_DATA_HOME", "data"), ("XDG_RUNTIME_DIR", "runtime")):
            (root / child).mkdir(mode=0o700)
            env[key] = str(root / child)
        env.update(CMUX_SHELL="/bin/sh", CMUX_LINUX_UI="parity", CMUX_SOCKET_PATH=address,
                   GDK_BACKEND="x11", GDK_SCALE="1", GDK_DPI_SCALE="1", GSK_RENDERER="cairo",
                   GTK_THEME="Adwaita:dark", GTK_A11Y="none", NO_AT_BRIDGE="1", LC_ALL="C.UTF-8")
        log_path = root / "app.log"
        with log_path.open("w") as log:
            app = subprocess.Popen([str(binary), "app", "--renderer", "gtk", "--socket", address],
                                   env=env, cwd=root, stdout=log, stderr=subprocess.STDOUT,
                                   start_new_session=True)
        x11 = None

        def rpc(method, **params):
            if app.poll() is not None:
                raise AssertionError(f"cmux exited with status {app.returncode}")
            with socket.socket(socket.AF_UNIX) as connection:
                connection.settimeout(5)
                connection.connect(address)
                connection.sendall((json.dumps({"id": "pointer-test", "method": method,
                                                "params": params}) + "\n").encode())
                with connection.makefile("rb") as response:
                    reply = json.loads(response.readline())
            if reply.get("error"):
                raise AssertionError(f"{method}: {reply['error']}")
            return reply["result"]

        def order(pane):
            return [row["surface_id"] for row in rpc("pane.surfaces", pane_id=pane)["surfaces"]]

        try:
            eventually("cmux socket did not appear", lambda: Path(address).exists() or app.poll() is not None)
            initial = rpc("surface.list")["surfaces"][0]
            alpha, left = initial["surface_id"], initial["pane_id"]
            rpc("surface.action", surface_id=alpha, action="rename", title="Alpha")
            beta = rpc("surface.create", pane_id=left, type="terminal", title="Beta", focus=False)["surface_id"]
            gamma = rpc("surface.create", pane_id=left, type="terminal", title="Gamma", focus=False)["surface_id"]
            split = rpc("surface.split", surface_id=alpha, direction="right", focus=False)
            delta, right = split["surface_id"], split["pane_id"]
            rpc("surface.action", surface_id=delta, action="rename", title="Delta")
            rpc("sidebar.left", action="hide")
            rpc("sidebar.right", action="hide")
            rpc("surface.focus", surface_id=alpha)
            x11 = X11()
            window = eventually("cmux X11 window did not appear", x11.app_window)
            x11.x.XResizeWindow(x11.display, window, 1200, 800)
            x11.x.XSetInputFocus(x11.display, window, 1, 0)
            x11.x.XFlush(x11.display)
            time.sleep(0.6)
            x, y, width, _ = x11.geometry(window)
            # Fixed scale/theme and hidden sidebars give two equal panes. The
            # compact titlebar/tab strip places tab centers 58px below the window.
            pane_width = width / 2
            tab_y = y + 58
            gamma_point = (x + pane_width * 0.70, tab_y)
            first_tab = (x + 24, tab_y)
            other_tab = (x + pane_width + 24, tab_y)

            # Prove the source coordinate hits Gamma before judging drag behavior.
            x11.click(gamma_point)
            eventually("fixture pointer did not select Gamma; tab geometry changed",
                       lambda: rpc("system.identify")["focused"]["surface_id"] == gamma)
            rpc("surface.focus", surface_id=alpha)
            rpc("surface.focus", surface_id=delta)
            time.sleep(0.3)
            x11.drag(gamma_point, first_tab)
            eventually(f"native reorder failed: expected Gamma, Alpha, Beta; got {order(left)}",
                       lambda: order(left) == [gamma, alpha, beta])

            x11.drag(first_tab, (x + pane_width + 100, y + 200))
            time.sleep(0.3)
            assert order(left) == [gamma, alpha, beta], "terminal-body drop moved a tab"
            assert order(right) == [delta], "terminal-body drop changed the destination pane"
            for surface in (alpha, beta, gamma, delta):
                text = rpc("surface.read_text", surface_id=surface)["text"]
                assert all(token not in text for token in (gamma, "CmuxPaneTabTransfer")), \
                    "terminal received internal tab drag content"

            x11.drag(first_tab, other_tab)
            eventually("native cross-pane drag did not move Gamma before Delta",
                       lambda: order(left) == [alpha, beta] and order(right) == [gamma, delta])
            assert rpc("system.identify")["focused"]["surface_id"] == gamma
            assert app.poll() is None, "cmux exited after dragging"
            print("PASS: native tab reorder, terminal-drop rejection, cross-pane move and focus")
        except Exception:
            if screenshot:
                try:
                    from PIL import ImageGrab
                    ImageGrab.grab(xdisplay=os.environ["DISPLAY"]).save(screenshot)
                    print(f"Failure screenshot: {screenshot}", flush=True)
                except Exception as error:
                    print(f"Screenshot unavailable: {error}", flush=True)
            print(log_path.read_text()[-4000:], flush=True)
            raise
        finally:
            if x11:
                x11.button(False)
                x11.x.XCloseDisplay(x11.display)
            try:
                os.killpg(app.pid, signal.SIGTERM)
            except ProcessLookupError:
                pass
            try:
                app.wait(timeout=3)
            except subprocess.TimeoutExpired:
                os.killpg(app.pid, signal.SIGKILL)
                app.wait()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument("--screenshot", type=Path)
    args = parser.parse_args()
    run(args.binary.resolve(strict=True), args.screenshot)
