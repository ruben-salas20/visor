#!/usr/bin/env python3
"""Screen region to an evenly timed frame sequence, with grim (there is no wf-recorder).

    capturar.py "x,y WxH" <dir> [--fps 15]   stops on SIGTERM or SIGINT

grim takes ~17 ms per frame. If one comes late, the sequence repeats the last frame
taken so that the rhythm stays real: frame k shows the screen at time k / fps.
"""
import argparse, os, signal, subprocess, time

ap = argparse.ArgumentParser()
ap.add_argument("geometry")
ap.add_argument("dir")
ap.add_argument("--fps", type=float, default=15)
a = ap.parse_args()
raw = os.path.join(a.dir, "raw")
seq = os.path.join(a.dir, "seq")
os.makedirs(raw, exist_ok=True)
os.makedirs(seq, exist_ok=True)

stop = False
def on_signal(*_):
    global stop
    stop = True
signal.signal(signal.SIGTERM, on_signal)
signal.signal(signal.SIGINT, on_signal)

step = 1 / a.fps
t0 = time.monotonic()
shots = []  # (time, file)
k = 0
while not stop:
    due = t0 + k * step
    now = time.monotonic()
    if now < due:
        time.sleep(due - now)
    f = os.path.join(raw, f"{len(shots):05d}.ppm")
    t = time.monotonic() - t0
    subprocess.run(["grim", "-t", "ppm", "-g", a.geometry, f], check=True)
    shots.append((t, f))
    k = int((time.monotonic() - t0) / step) + 1

# Frame n is the last shot taken at or before n * step.
n, j = 0, 0
end = shots[-1][0]
while n * step <= end:
    while j + 1 < len(shots) and shots[j + 1][0] <= n * step:
        j += 1
    os.link(shots[j][1], os.path.join(seq, f"{n:05d}.ppm"))
    n += 1
late = sum(1 for (t1, _), (t2, _) in zip(shots, shots[1:]) if t2 - t1 > step * 1.5)
print(f"{len(shots)} capturas, {n} fotogramas, {late} huecos de más de 1,5 fotogramas")
