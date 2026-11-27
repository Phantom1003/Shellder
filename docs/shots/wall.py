#!/usr/bin/env python3
"""Draws the demo desk's wallpaper, docs/site/demo/wall.jpg: a manta ray from
below, the water's surface behind it.

    docs/shots/wall.py [OUT] [--quick]

Needs numpy, scipy and Pillow (a venv of your own: nothing else in the
repository uses them). A minute for the picture, 2400 x 1520; --quick makes
one of half the size in a few seconds, to look at a change. The picture is
checked in: the site's build takes it as it is and does not run this.

Nothing is traced from a photograph, everything is computed. The surface is
a field of waves, and each pixel that looks up at it is bent through it as
the light is: into the sky, bright about the sun, or, where the surface lies
too flat to the eye, back down into the deep (the bright part's edge is that
angle, not a line drawn). The ray is a surface with a thickness, its wings
bent, curled at the tip and taken through a stroke, its belly's marks laid
over its outline; it is drawn in perspective with a depth buffer, a few
times over through the beat of its wings, which is the blur at their tips.
The water between takes the red out and puts its own blue in with distance,
and the lens adds what a lens does: the shafts of light, what floats near
it, the dark corners and the grain. The background is drawn out a little
along the way the ray swims, as for a camera that goes with it.

The numbers that pose the ray and light the water can be given in the
environment to try others (HEADING, ROLL, NOSE, TIP_L, CURL_R, WAVE, SUN,
EXPOSE and the rest: see knob()).
"""
import os
import sys
import time
import warnings

import numpy as np
from numpy.fft import rfft2, irfft2, fftfreq, rfftfreq
from scipy import ndimage as ndi
from PIL import Image

# numpy's matrix product on macOS (Accelerate) warns of overflows that are not there
warnings.filterwarnings("ignore", message=".*encountered in matmul")

QUICK = "--quick" in sys.argv
OUT = ([a for a in sys.argv[1:] if not a.startswith("--")] or ["docs/site/demo/wall.jpg"])[0]
W, H = (1200, 760) if QUICK else (2400, 1520)
SS = 2
N = 2048 if QUICK else 4096          # the tiled textures, texels a side
PERIOD = 16.0                        # and metres a side
TEXEL = PERIOD / N
F32 = np.float32
T0 = time.time()


def say(*a):
    print(f"[{time.time() - T0:6.1f}s]", *a, flush=True)


def knob(name, default):
    """A number of the picture's, which the environment may set."""
    return float(os.environ.get(name, default))


# ---------------------------------------------------------------- noise

def spectral(n, lam_lo, lam_hi, beta, seed, period=PERIOD, aniso=None):
    """Noise of wavelengths lam_lo..lam_hi (metres), periodic over `period`."""
    r = np.random.default_rng(seed)
    f = rfft2(r.standard_normal((n, n), dtype=F32))
    ky = fftfreq(n, d=period / n)[:, None]
    kx = rfftfreq(n, d=period / n)[None, :]
    k = np.hypot(kx, ky)
    k[0, 0] = 1
    amp = k ** (-beta / 2)
    amp = amp / (1 + (1 / (lam_hi * k)) ** 4) / (1 + (k * lam_lo) ** 4)
    if aniso:
        th, a = aniso
        c = (kx * np.cos(th) + ky * np.sin(th)) / k
        amp = amp * ((1 - a) + a * c * c)
    amp[0, 0] = 0
    out = irfft2(f * amp, s=(n, n)).astype(F32)
    return out / out.std()


def grad(a, d):
    return ((np.roll(a, -1, 1) - np.roll(a, 1, 1)) / (2 * d),
            (np.roll(a, -1, 0) - np.roll(a, 1, 0)) / (2 * d))


def smooth(a, lo, hi):
    t = np.clip((a - lo) / (hi - lo), 0, 1)
    return t * t * (3 - 2 * t)


def sample(tex, u, v):
    """Bilinear, periodic; u, v in texels."""
    n = tex.shape[0]
    x0 = np.floor(u)
    y0 = np.floor(v)
    fx = (u - x0).astype(F32)
    fy = (v - y0).astype(F32)
    x0 = x0.astype(np.int64) % n
    y0 = y0.astype(np.int64) % n
    x1 = (x0 + 1) % n
    y1 = (y0 + 1) % n
    if tex.ndim == 3:
        fx = fx[..., None]
        fy = fy[..., None]
    return ((tex[y0, x0] * (1 - fx) + tex[y0, x1] * fx) * (1 - fy)
            + (tex[y1, x0] * (1 - fx) + tex[y1, x1] * fx) * fy)


# ---------------------------------------------------------------- the scene

CAM = np.array([0.0, 0.0, -knob("CAM_DEPTH", 4.2)])
PITCH = np.radians(knob("PITCH", 46))
HFOV = np.radians(78)
FWD = np.array([0, np.cos(PITCH), np.sin(PITCH)])
RIGHT = np.array([1.0, 0, 0])
UP = np.array([0, -np.sin(PITCH), np.cos(PITCH)])
TANH = np.tan(HFOV / 2)
TANV = TANH * H / W

RAY_AT = np.array([knob("RAY_X", 0.25), knob("RAY_Y", 1.9), -knob("RAY_DEPTH", 2.1)])   # the ray's middle, metres
HEADING = np.radians(knob("HEADING", 235))
SPAN = 1.65                   # half the ray's width, metres

SUN_T = np.array([0.42, -0.90]) / np.hypot(0.42, 0.90) * np.tan(np.radians(knob("SUN", 13)))   # light's drift a metre down
SUN_L = np.array([-SUN_T[0], -SUN_T[1], 1.0])
SUN_L /= np.linalg.norm(SUN_L)                                                  # towards the sun

KD = np.array([0.24, 0.060, 0.034], F32)     # downwelling light lost a metre
CV = np.array([0.25, 0.105, 0.058], F32)     # contrast lost a metre of view
DEEP_DOWN = np.array([0.008, 0.070, 0.190], F32)
DEEP_LEVEL = np.array([0.010, 0.115, 0.330], F32)


DEEP_UP = np.array([0.050, 0.330, 0.600], F32)


def water_colour(dz):
    """The water's own light, looking down (-1), level (0) or up (1)."""
    t = np.clip(dz + 1, 0, 1)[..., None]
    u = np.clip(dz, 0, 1)[..., None] ** 0.8
    return (DEEP_DOWN * (1 - t) + DEEP_LEVEL * t) * (1 - u) + DEEP_UP * u


# ---------------------------------------------------------------- the surface and its light

def caustics(gx, gy, shift):
    """Light a level of water gets through the waves: every texel's ray
    moved by `shift` texels a unit of slope, and counted where it lands."""
    n = gx.shape[0]
    out = np.zeros(n * n, np.float64)
    step = 512
    for a in range(0, n, step):
        yy, xx = np.mgrid[a:a + step, 0:n].astype(F32)
        px = xx + shift * gx[a:a + step]
        py = yy + shift * gy[a:a + step]
        x0 = np.floor(px)
        y0 = np.floor(py)
        fx = px - x0
        fy = py - y0
        x0 = x0.astype(np.int64) % n
        y0 = y0.astype(np.int64) % n
        x1 = (x0 + 1) % n
        y1 = (y0 + 1) % n
        for xi, yi, w in ((x0, y0, (1 - fx) * (1 - fy)), (x1, y0, fx * (1 - fy)),
                          (x0, y1, (1 - fx) * fy), (x1, y1, fx * fy)):
            out += np.bincount((yi * n + xi).ravel(), w.ravel().astype(np.float64), n * n)
    return out.reshape(n, n).astype(F32)



def mips(tex, levels=4):
    out = [tex]
    for _ in range(levels - 1):
        t = out[-1]
        out.append((t[0::2, 0::2] + t[1::2, 0::2] + t[0::2, 1::2] + t[1::2, 1::2]) / 4)
    return out


def body_frame():
    c, s = np.cos(HEADING), np.sin(HEADING)
    fwd = np.array([c, s, 0.0])
    right = np.array([s, -c, 0.0])
    up = np.array([0, 0, 1.0])
    roll = np.radians(knob("ROLL", -42))
    pitch = np.radians(knob("NOSE", 10))
    # nose up, then banked
    fwd, up = fwd * np.cos(pitch) + up * np.sin(pitch), up * np.cos(pitch) - fwd * np.sin(pitch)
    right, up = right * np.cos(roll) + up * np.sin(roll), up * np.cos(roll) - right * np.sin(roll)
    return right, fwd, up


def tube(centre, ra, rb, nt, nphi, updir=np.array([0, 0, 1.0])):
    """A tapering tube about a centre line (nt, 3); ra across, rb up."""
    tan = np.gradient(centre, axis=0)
    tan /= np.linalg.norm(tan, axis=1, keepdims=True)
    side = np.cross(tan, updir)
    side /= np.linalg.norm(side, axis=1, keepdims=True)
    up = np.cross(side, tan)
    phi = np.linspace(0, 2 * np.pi, nphi)[None, :, None]
    ca, sa = np.cos(phi), np.sin(phi)
    P = centre[:, None] + ra[:, None, None] * ca * side[:, None] + rb[:, None, None] * sa * up[:, None]
    nrm = ca * side[:, None] / np.maximum(ra, 1e-4)[:, None, None] + sa * up[:, None] / np.maximum(rb, 1e-4)[:, None, None]
    nrm /= np.linalg.norm(nrm, axis=-1, keepdims=True)
    return P, nrm


def project(P):
    v = P - CAM
    zc = v @ FWD
    px = (v @ RIGHT / zc / TANH + 1) / 2 * W * SS
    py = (1 - v @ UP / zc / TANV) / 2 * H * SS
    return px, py, zc


def halve(a):
    return (a[0::2, 0::2] + a[1::2, 0::2] + a[0::2, 1::2] + a[1::2, 1::2]) / 4


def streak(img, angle, length):
    """A blur along one way: a line of `length` pixels at `angle` (degrees, up from the right)."""
    if length < 1.5:
        return img
    n = int(np.ceil(length))
    k = np.zeros((2 * n + 1, 2 * n + 1), F32)
    for t in np.linspace(-length / 2, length / 2, 8 * n):
        x = n + t * np.cos(np.radians(angle))
        y = n - t * np.sin(np.radians(angle))
        k[int(round(y)), int(round(x))] += 1
    k = ndi.gaussian_filter(k, 0.6)
    k /= k.sum()
    if img.ndim == 2:
        return ndi.convolve(img, k, mode="nearest")
    return np.dstack([ndi.convolve(img[..., c], k, mode="nearest") for c in range(img.shape[2])])


def textures():
    say("waves")
    wind = np.radians(25)
    eta = (spectral(N, 0.38, 4.0, 3.0, 11, aniso=(wind, 0.5))
           + 0.035 * spectral(N, 0.14, 0.38, 3.0, 12, aniso=(wind + 0.5, 0.4)))
    gx, gy = grad(eta, TEXEL)
    lap = grad(gx, TEXEL)[0] + grad(gy, TEXEL)[1]
    # the waves' height is set by how they focus two metres down
    k = knob("FOCUS", 1.0) / (0.25 * 2.0 * lap.std())
    gx *= k
    gy *= k
    say("light at the ray")
    c_ray = caustics(gx, gy, 0.25 * 2.0 / TEXEL)
    c_ray = ndi.gaussian_filter(c_ray, 0.008 / TEXEL, mode="wrap")
    # the small ripples the wind lays over the waves: they do nothing to the
    # light below, and are most of what the surface looks like from under it
    fine = spectral(N, 0.07, 0.60, 3.2, 13, aniso=(wind + 0.2, 0.78))
    fx, fy = grad(fine, TEXEL)
    k = knob("CHOP", 0.075) / np.sqrt((fx * fx + fy * fy).mean())
    calm = knob("SWELL", 0.085) / np.sqrt((gx * gx + gy * gy).mean())
    slope = np.dstack([calm * gx + k * fx, calm * gy + k * fy]).astype(F32)
    say("slopes: waves", float(np.sqrt((gx * gx + gy * gy).mean())), "with ripples", float(np.sqrt((slope ** 2).sum(-1).mean())))
    return c_ray, slope


def wing(side, tip, lag, top, ns, nq, curl=0.0, wave=0.0, phase=0.0, sweep=0.0):
    """One wing's upper or lower skin: points (ns, nq, 3) in the ray's own
    frame, half a width being 1, with their outward normals."""
    s = np.linspace(0, 1, ns)[:, None]
    q = np.linspace(0, 1, nq)[None, :]
    xi = (1 - np.cos(np.pi * q)) / 2
    head = np.exp(-(s / 0.185) ** 4)
    yle = 0.33 - 0.40 * s ** 1.30 - 0.06 * s ** 7 + 0.045 * head - 0.012 * np.exp(-(s / 0.10) ** 2)
    yte = -0.53 + 0.455 * s ** 0.82 - 0.055 * s ** 7 + 0.035 * np.exp(-(s / 0.06) ** 2)
    chord = np.maximum(yle - yte, 0)
    # the stroke takes the wing fore or aft as well as up or down
    y = yle - xi * chord + sweep * s ** 2.2
    thick = 0.21 * np.exp(-(s / 0.26) ** 2) + 0.060 * (1 - s) ** 0.9 + 0.002
    thick = thick * np.minimum(1, chord / 0.25) ** 0.6
    prof = xi ** 0.5 * (1 - xi) ** 1.15
    prof = prof / prof.max()
    # the belly is the flatter side, and fuller towards the head
    belly = xi ** 0.42 * (1 - xi) ** 1.3
    belly = belly / belly.max()
    zl = thick * (prof * 0.66 if top else -belly * 0.34)
    # the beat runs through the wing from its fore edge to its hind one: the hind edge is behind the fore
    zl = zl + wave * s ** 1.25 * np.sin(phase - 2.4 * xi) * np.minimum(1, chord / 0.22) * smooth(s, 0.08, 0.30)
    # the wing bends more the further out, and its tip curls on
    th = tip * s ** 1.55 + curl * smooth(s, 0.55, 1.0) ** 1.5 + 0 * xi
    ds = 1 / (ns - 1)
    x = np.cumsum(np.cos(th), 0) * ds - ds
    zc = np.cumsum(np.sin(th), 0) * ds + lag * s ** 1.2 * (xi - 0.35) * chord
    X = x - zl * np.sin(th)
    Z = zc + zl * np.cos(th)
    P = np.stack([side * X, y + 0 * X, Z], -1)
    d1 = np.gradient(P, axis=0)
    d2 = np.gradient(P, axis=1)
    nrm = np.cross(d1, d2)
    nrm /= np.linalg.norm(nrm, axis=-1, keepdims=True) + 1e-12
    out = np.stack([-np.sin(th) * side, 0 * th, np.cos(th)], -1) * (1 if top else -1)
    nrm *= np.sign((nrm * out).sum(-1, keepdims=True) + 1e-9)
    w = smooth(chord + 0 * xi, 0.0, 0.07)[..., None]
    nrm = nrm * w + out * (1 - w)
    nrm /= np.linalg.norm(nrm, axis=-1, keepdims=True)
    plan = np.stack([side * s + 0 * xi, y + 0 * X], -1)
    return P, nrm, plan, np.broadcast_to(xi, X.shape), np.broadcast_to(s, X.shape), thick * prof


def plan_tex(tex, plan):
    """A texture laid over the ray's outline: 2 units a side."""
    n = tex.shape[0]
    return sample(tex, (plan[..., 0] + 1) / 2 * n, (plan[..., 1] + 1) / 2 * n)


def skin(plan, xi, s, top, tex):
    """Albedo of the ray's skin at a place of its outline (x across, y along)."""
    xs = plan[..., 0]
    x = np.abs(xs)
    y = plan[..., 1]
    mott = plan_tex(tex["mott"], plan)
    fine = plan_tex(tex["fine"], plan)
    if top:
        a = 0.030 + 0.004 * mott + 0.003 * fine
        t = np.clip((x - 0.05) / 0.47, 0, 1)
        spine = 0.185 - 0.08 * t - 0.10 * t * t + 0.004 * mott
        half = 0.012 + 0.085 * np.sin(np.pi * t ** 0.75) ** 1.3
        d = (y - spine) / half
        band = np.where(d > 0, smooth(d, 1.0, 0.55), smooth(d, -1.25, -0.1))
        band = band * smooth(x, 0.05, 0.10) * smooth(x, 0.55, 0.38)
        a = a + band * 0.17
        return np.stack([a * 0.96, a, a * 1.05], -1).astype(F32)
    # the belly: white, greying to the hind edge and the tips
    a = 0.72 + 0.055 * mott + 0.012 * fine
    edge = smooth(xi, 0.62, 1.0) * smooth(s, 0.10, 0.45)
    a = a * (1 - 0.50 * edge ** 1.4) * (1 - 0.42 * smooth(s, 0.72, 1.0))
    # the mouth, a dark slot across the head's front
    mouth = smooth(xi, 0.085, 0.035) * smooth(x, 0.165, 0.135)
    # the gill slits, five a side, each a bow across its half of the chest
    gills = 0 * a
    ridge = 0 * a
    for i in range(5):
        x0, x1 = 0.050 + 0.004 * i, 0.185 - 0.017 * i
        u = np.clip((x - x0) / (x1 - x0), 0, 1)
        yy = 0.215 - 0.058 * i - 0.045 * u + 0.05 * u * (1 - u) * 4 * 0.5
        on = smooth(x, x0 - 0.004, x0 + 0.01) * smooth(x, x1 + 0.004, x1 - 0.012)
        d = y - yy
        gills = np.maximum(gills, smooth(np.abs(d), 0.0050, 0.0008) * on)
        ridge = ridge + (smooth(d, -0.035, -0.003) - 1) * on * smooth(d, 0.001, -0.003) + 0.5 * (1 - smooth(d, 0.002, 0.022)) * on * smooth(d, -0.001, 0.003)
    # the dark patch between the gills and behind them, and the spots about it
    blot = plan_tex(tex["blot"], plan)
    spots = plan_tex(tex["spots"], plan)
    near = np.exp(-((xs - 0.015) / 0.15) ** 2 - ((y + 0.055) / 0.085) ** 2)
    near = np.maximum(near, 0.9 * np.exp(-((xs + 0.11) / 0.06) ** 2 - ((y + 0.10) / 0.05) ** 2))
    patch = smooth(near * 1.5 + 0.38 * blot + 0.10 * spots, 0.86, 1.36)
    field = np.exp(-(xs / 0.42) ** 2 - ((y + 0.16) / 0.19) ** 2) + 0.55 * np.exp(-((x - 0.42) / 0.22) ** 2 - ((y + 0.08) / 0.10) ** 2)
    dots = smooth(spots + 1.25 * field + 0.35 * blot, 2.22, 2.74) * (0.72 + 0.28 * smooth(mott, -1.0, 1.0))
    dark = np.clip(np.maximum.reduce([patch * 0.93, dots * 0.88, gills * 0.80, mouth * 0.90]), 0, 1)
    a = a * (1 + 0.13 * ridge) * (1 - dark) + 0.055 * dark
    return np.stack([a, a, a * 0.985], -1).astype(F32)


def ray_parts(tip_l, tip_r, lag, quick_div=1):
    parts = []
    for side, tip in ((-1, tip_l), (1, tip_r)):
        for top in (True, False):
            ns, nq = (1300 // quick_div, 1000 // quick_div) if top else (2700 // quick_div, 2100 // quick_div)
            L = side < 0
            P, nrm, plan, xi, s, thick = wing(side, tip, lag, top, ns, nq,
                                              curl=np.radians(knob("CURL_L" if L else "CURL_R", -14 if L else 28)),
                                              wave=knob("WAVE", 0.055),
                                              phase=knob("PHASE_L" if L else "PHASE_R", 3.4 if L else 0.9),
                                              sweep=knob("SWEEP_L" if L else "SWEEP_R", -0.06 if L else 0.07))
            parts.append(dict(P=P, n=nrm, plan=plan, xi=xi, s=s, top=top, kind=0, thick=thick))
    # the head's two lobes, let down: blades that stand on edge, ahead of the mouth
    t = np.linspace(0, 1, 900 // quick_div)
    for side, furl in ((-1, knob("LOBE_L", 0.25)), (1, knob("LOBE_R", 0.8))):
        # furl 0: a paddle held out and down; 1: rolled to a horn pointing ahead
        c = np.stack([side * (0.172 + (1 - furl) * 0.035 * np.sin(np.pi * t) - 0.05 * furl * t ** 1.8),
                      0.285 + (0.235 - 0.03 * furl) * t,
                      0.0 - (1 - furl) * 0.11 * t ** 1.2 - 0.03 * furl * t], 1)
        end = np.clip(1 - t ** 4, 0, 1) ** 0.6
        wide = (0.016 + 0.02 * furl) * (1 - 0.3 * t) * end + 1e-4
        tall = (0.070 - 0.025 * furl) * (0.80 + 0.4 * np.sin(np.pi * t ** 0.8) * (1 - furl)) * end + 1e-4
        P, nrm = tube(c, wide, tall, len(t), 520 // quick_div)
        plan = P[..., :2]
        inner = np.broadcast_to(np.linspace(0, 2 * np.pi, P.shape[1])[None, :], P.shape[:2])
        parts.append(dict(P=P, n=nrm, plan=plan, xi=inner, s=np.broadcast_to(t[:, None], P.shape[:2]), top=False, kind=1,
                          thick=np.broadcast_to(wide[:, None], P.shape[:2]), side=side))
    # the tail
    t = np.linspace(0, 1, 2600 // quick_div)
    c = np.stack([0.16 * t ** 1.7 - 0.05 * np.sin(3.2 * t), -0.47 - 0.62 * t, 0.0 - 0.10 * t ** 1.5], 1)
    r = 0.0065 * (1 - t) ** 0.8 + 0.0016
    P, nrm = tube(c, r, r, len(t), 40)
    parts.append(dict(P=P, n=nrm, plan=P[..., :2], xi=None, s=None, top=True, kind=2, thick=np.full(P.shape[:2], 0.05)))
    # the pelvic fins, two small flaps beside the tail's root
    t = np.linspace(0, 1, 220 // quick_div)
    for side in (-1, 1):
        c = np.stack([side * (0.035 + 0.02 * t), -0.455 - 0.075 * t, -0.012 - 0.01 * t], 1)
        end = np.sin(np.pi * np.clip(t * 0.9 + 0.1, 0, 1)) ** 0.6
        P, nrm = tube(c, 0.022 * end + 1e-4, 0.007 * end + 1e-4, len(t), 100 // quick_div)
        parts.append(dict(P=P, n=nrm, plan=P[..., :2], xi=None, s=None, top=False, kind=3, thick=np.full(P.shape[:2], 0.02)))
    return parts


def draw_ray(c_ray, tex, tips, lag):
    """The ray into colour and coverage buffers of the oversampled frame."""
    right, fwd, up = body_frame()
    M = np.stack([right, fwd, up], 1) * SPAN            # the ray's frame to the world
    hs, ws = H * SS, W * SS
    cols, deps, idxs = [], [], []
    sky_tint = np.array([0.80, 1.0, 1.08], F32)
    deep_tint = np.array([0.45, 0.95, 1.10], F32)
    for part in ray_parts(*tips, lag, 2 if QUICK else 1):
        P, nrm, plan, top, kind = part["P"], part["n"], part["plan"], part["top"], part["kind"]
        Pw = P.reshape(-1, 3) @ M.T + RAY_AT
        nw = nrm.reshape(-1, 3) @ (M.T / SPAN)
        px, py, zc = project(Pw)
        view = CAM - Pw
        dist = np.linalg.norm(view, axis=1)
        view /= dist[:, None]
        facing = (nw * view).sum(1)
        depth = -Pw[:, 2]
        if kind == 0:
            alb = skin(plan, part["xi"], part["s"], top, tex).reshape(-1, 3)
        elif kind == 1:
            # a lobe: white within, where it faces the other, grey without, dark at its end
            inward = np.cos(part["xi"]) * -part["side"]
            a = (0.42 + 0.30 * smooth(inward, -0.3, 0.5)) * (1 - 0.6 * smooth(part["s"], 0.72, 1.0))
            alb = np.repeat(a.reshape(-1, 1), 3, 1).astype(F32)
        elif kind == 2:
            alb = np.zeros((len(Pw), 3), F32) + np.array([0.05, 0.055, 0.06], F32)
        else:
            alb = np.zeros((len(Pw), 3), F32) + 0.55
        # the light: the sun through the waves, on what faces it
        cu = (Pw[:, 0] - SUN_T[0] * depth) / TEXEL
        cv = (Pw[:, 1] - SUN_T[1] * depth) / TEXEL
        light = sample(c_ray, cu, cv)
        light = light / (1 + 0.13 * light) * 1.13
        ndl = np.clip(nw @ SUN_L, 0, 1)
        e = np.exp(-KD * depth[:, None])
        # and the water's own, far more from above than from below
        nz = nw[:, 2:3]
        side_on = 1 - nz * nz
        amb = e * (sky_tint * (0.5 + 0.5 * nz) ** 1.5 * 0.95
                   + deep_tint * ((0.5 - 0.5 * nz) ** 1.2 * AMB_BELOW + side_on * AMB_SIDE))
        # light from ahead of the camera's view, low: what the water sends back along the way it came
        toward = np.clip((nw * view).sum(1), 0, 1)[:, None]
        amb = amb + e * deep_tint * FILL * toward ** 2
        # a thin wing lets the day through
        thin = np.exp(-part["thick"].reshape(-1, 1) * SPAN / 0.035)
        through = e * sky_tint * (0.35 + 0.25 * light[:, None]) * thin * (0.5 - 0.5 * nz) * GLOW
        hv = view + SUN_L
        hv /= np.linalg.norm(hv, axis=1, keepdims=True)
        ndh = np.clip((nw * hv).sum(1), 0, 1)
        spec = ((0.5 * ndh ** 120 + 0.015 * ndh ** 10) * light * ndl)[:, None] * e
        col = alb * (0.80 * (light * ndl)[:, None] * e + amb + through) + spec * 0.55
        tr = np.exp(-CV * dist[:, None])
        col = col * tr + water_colour(-view[:, 2]) * (1 - tr)
        ok = (px >= 1) & (px < ws - 1) & (py >= 1) & (py < hs - 1) & (zc > 0.2) & (facing > -0.25)
        col = col[ok].astype(F32)
        ix, iy = np.round(px[ok]).astype(np.int64), np.round(py[ok]).astype(np.int64)
        for ox in (-1, 0):
            for oy in (-1, 0):
                cols.append(col)
                deps.append(zc[ok].astype(F32))
                idxs.append((iy + oy) * ws + ix + ox)
    col = np.concatenate(cols)
    dep = np.concatenate(deps)
    idx = np.concatenate(idxs)
    order = np.argsort(-dep, kind="stable")
    rgb = np.zeros((hs * ws, 3), F32)
    cover = np.zeros(hs * ws, bool)
    rgb[idx[order]] = col[order]
    cover[idx] = True
    rgb = rgb.reshape(hs, ws, 3)
    cover = cover.reshape(hs, ws)
    if not cover.any():
        return rgb, cover.astype(F32)
    ys, xs = np.nonzero(cover)
    y0, y1, x0, x1 = max(ys.min() - 4, 0), ys.max() + 5, max(xs.min() - 4, 0), xs.max() + 5
    c = cover[y0:y1, x0:x1]
    closed = ndi.binary_closing(c, np.ones((3, 3), bool), iterations=2)
    holes = closed & ~c
    if holes.any():
        _, (iy, ix) = ndi.distance_transform_edt(~c, return_indices=True)
        sub_rgb = rgb[y0:y1, x0:x1]
        sub_rgb[holes] = sub_rgb[iy[holes], ix[holes]]
        cover[y0:y1, x0:x1] = closed
    return rgb, cover.astype(F32)


AMB_BELOW = knob("AMB_BELOW", 0.23)
AMB_SIDE = knob("AMB_SIDE", 0.22)
FILL = knob("FILL", 0.10)
GLOW = knob("GLOW", 1.0)
SUN_AIR = None


def sky(t):
    """What the air sends down along -t (t: unit vectors up into the air)."""
    global SUN_AIR
    if SUN_AIR is None:
        # the sun's own place in the air: its light, bent at a flat surface, comes down along SUN_L
        s = np.sin(np.arccos(SUN_L[2])) * 1.333
        h = SUN_L[:2] / np.linalg.norm(SUN_L[:2])
        SUN_AIR = np.array([h[0] * s, h[1] * s, np.sqrt(1 - s * s)])
    c = np.clip(t @ SUN_AIR, -1, 1)
    ang = np.degrees(np.arccos(c))
    up = np.clip(t[..., 2], 0, 1)[..., None]
    blue = np.array([0.30, 0.50, 0.92], F32) * (0.30 + 0.70 * up ** 1.5) + np.array([0.30, 0.36, 0.42], F32) * (1 - up) ** 2
    glow = (26 * np.exp(-(ang / 3.0) ** 2) + 3.0 * np.exp(-(ang / 12) ** 2) + 1.5 * np.exp(-ang / 34))[..., None]
    return blue * 0.95 + glow * np.array([1.0, 0.97, 0.90], F32)


def draw_surface(slope):
    """The surface from under it, a pixel at a time: through each bit of it
    the sky, bent, or, where it lies too flat to the eye, the deep mirrored."""
    hs, ws = H * SS, W * SS
    out = np.zeros((H, W, 3), F32)
    levels = mips(slope, 5)
    rows = 160
    xs = ((np.arange(ws) + 0.5) / ws * 2 - 1) * TANH
    for a in range(0, hs, rows):
        ys = (1 - (np.arange(a, min(a + rows, hs)) + 0.5) / hs * 2) * TANV
        d = FWD[None, None, :] + xs[None, :, None] * RIGHT + ys[:, None, None] * UP
        d = d / np.linalg.norm(d, axis=-1, keepdims=True)
        dz = d[..., 2]
        upward = dz > 0.02
        t = np.where(upward, -CAM[2] / np.maximum(dz, 0.02), 1e4)
        X = CAM[0] + t * d[..., 0]
        Y = CAM[1] + t * d[..., 1]
        dist = np.minimum(t, 400).astype(F32)
        foot = dist * (2 * TANH / ws) / TEXEL / np.sqrt(np.maximum(dz, 0.02))
        lod = np.clip(np.log2(np.maximum(foot, 1e-3)) - 0.8, 0, len(levels) - 1.001)
        l0 = np.floor(lod).astype(int)
        f = (lod - l0)[..., None].astype(F32)
        u, v = X / TEXEL, Y / TEXEL
        g = np.zeros(X.shape + (2,), F32)
        for i in range(len(levels)):
            for j, wgt in ((i, 1 - f), (i - 1, f)):
                m = l0 == j
                if j < 0 or not m.any():
                    continue
                sc = 2 ** i
                g[m] += wgt[m] * sample(levels[i], u[m] / sc - 0.5 + 0.5 / sc, v[m] / sc - 0.5 + 0.5 / sc)
        # far off the ripples run together under a pixel: what is left of them is less steep
        n = np.dstack([-g[..., 0], -g[..., 1], np.ones_like(dz)])
        n /= np.linalg.norm(n, axis=-1, keepdims=True)
        cosi = np.clip((d * n).sum(-1), 0, 1)
        sin2 = 1.333 ** 2 * (1 - cosi ** 2)
        inside = sin2 < 1
        cost = np.sqrt(np.clip(1 - sin2, 0, 1))
        tdir = 1.333 * d + (cost - 1.333 * cosi)[..., None] * n
        # Fresnel, unpolarised
        rs = ((1.333 * cosi - cost) / (1.333 * cosi + cost + 1e-9)) ** 2
        rp = ((1.333 * cost - cosi) / (1.333 * cost + cosi + 1e-9)) ** 2
        refl = np.maximum(np.where(inside, np.clip((rs + rp) / 2, 0, 1), 1.0), smooth(sin2, 0.90, 1.04))[..., None]
        rdir = d - 2 * (d * n).sum(-1, keepdims=True) * n
        mirrored = water_colour(np.clip(rdir[..., 2], -1, 0)) * 0.9
        through = sky(tdir) / 1.333 ** 2 * 1.9
        # a facet leaning to the sun shows more of the bright sky about it
        lean = -(g[..., 0] * SUN_L[0] + g[..., 1] * SUN_L[1]) / np.hypot(SUN_L[0], SUN_L[1])
        through = through * np.clip(1 + 2.4 * lean, 0.45, 1.9)[..., None]
        col = through * (1 - refl) + mirrored * refl
        tr = np.exp(-CV * dist[..., None])
        col = col * tr + water_colour(dz) * (1 - tr)
        col = np.where(upward[..., None], col, water_colour(dz))
        out[a // SS:a // SS + col.shape[0] // SS] = halve(col)
    return out


def main():
    c_ray, slope = textures()
    tex = dict(mott=spectral(2048, 0.03, 0.5, 2.6, 41, 2.0), fine=spectral(2048, 0.004, 0.03, 2.0, 42, 2.0),
               blot=spectral(2048, 0.05, 0.45, 2.4, 43, 2.0), spots=spectral(2048, 0.035, 0.11, 1.4, 44, 2.0))

    subs = 1 if QUICK else int(knob("SUBS", 5))
    tip_l, tip_r = np.radians(knob("TIP_L", -14)), np.radians(knob("TIP_R", 34))
    beat = np.radians(knob("BEAT", 4.5))
    rgb = np.zeros((H, W, 3), F32)
    cov = np.zeros((H, W), F32)
    for i in range(subs):
        say("the ray", i)
        ph = (i - (subs - 1) / 2) / max(subs - 1, 1)
        c, m = draw_ray(c_ray, tex, (tip_l + beat * ph, tip_r + beat * ph), 0.30 + 0.5 * beat * ph)
        rgb += halve(c * m[..., None])
        cov += halve(m)
    rgb /= subs
    cov /= subs

    say("the surface")
    back = draw_surface(slope)
    right, fwd, up = body_frame()
    a = project(RAY_AT[None] - 0.5 * fwd[None])
    b = project(RAY_AT[None] + 0.5 * fwd[None])
    swim = np.degrees(np.arctan2(-(b[1][0] - a[1][0]), b[0][0] - a[0][0]))
    say("swims at", swim, "degrees on the picture")
    back = streak(back, swim, knob("PAN", 9) * W / 2400)
    sun = project(CAM[None] + SUN_L[None] * 1000.0)
    sx, sy = sun[0][0] / SS, sun[1][0] / SS
    say("the sun stands at", sx, sy, "on the picture")
    yy, xx = np.mgrid[0:H, 0:W].astype(F32)
    ang = np.arctan2(yy - sy, xx - sx)
    rad = np.hypot(xx - sx, yy - sy) / W
    fan = spectral(1024, 0.02, 0.5, 2.2, 51, 4.0)
    beams = ndi.gaussian_filter(sample(fan, ang * 1024 / 2.6, rad * 14 + 0 * ang), 3 * W / 2400)
    beams = smooth(beams, 0.1, 2.0)
    bright = (back * np.array([0.2, 0.7, 0.1], F32)).sum(-1)
    reach = smooth(rad, 0.15, 0.5) * smooth(bright, 0.75, 0.25)
    shafts = knob("SHAFTS", 0.22) * beams * reach
    back = back * (1 + shafts[..., None] * np.array([0.8, 1.0, 1.0], F32))
    pic = back * (1 - cov[..., None]) + rgb * (1 + 0.25 * shafts[..., None])

    # what floats in the water, near the lens
    r = np.random.default_rng(5)
    snow = np.zeros((H, W), F32)
    for size, count, gain in ((0.7, 2600, 0.05), (1.8, 420, 0.035), (5.0, 60, 0.016), (11.0, 12, 0.008)):
        layer = np.zeros((H, W), F32)
        layer[r.integers(0, H, count * (W * H) // (2400 * 1520)), r.integers(0, W, count * (W * H) // (2400 * 1520))] = 1
        layer = ndi.gaussian_filter(layer, size * W / 2400)
        snow += layer / layer.max() * gain
    pic += snow[..., None] * np.array([0.75, 1.0, 1.05], F32)

    # the lens and the sensor
    pic *= knob("EXPOSE", 1.02)
    yy, xx = np.mgrid[0:H, 0:W].astype(F32)
    rad = np.hypot((xx - W / 2) / (W / 2), (yy - H / 2) / (W / 2))
    pic *= (1 - 0.16 * rad ** 2.2)[..., None]
    glow = ndi.gaussian_filter(np.clip(pic - 0.75, 0, None), (14 * W / 2400, 14 * W / 2400, 0))
    pic = pic + 0.35 * glow
    pic = ndi.gaussian_filter(pic, (0.55 * W / 2400, 0.55 * W / 2400, 0))
    pic = pic * (1 + pic / 6.5) / (1 + pic)            # the shoulder of the curve
    pic = pic * 1.42
    pic = np.where(pic <= 0.0031308, pic * 12.92, 1.055 * np.clip(pic, 1e-6, None) ** (1 / 2.4) - 0.055)
    noise = r.standard_normal((H, W, 3)).astype(F32)
    noise = 0.7 * noise[..., :1] + 0.3 * noise
    noise = ndi.gaussian_filter(noise, (0.5, 0.5, 0))
    pic = pic + noise * (0.016 - 0.008 * pic)
    Image.fromarray((pic.clip(0, 1) * 255 + r.uniform(-.5, .5, pic.shape)).round().clip(0, 255).astype(np.uint8)).save(
        OUT, quality=84, optimize=True, progressive=True)
    say("wrote", OUT)


if __name__ == "__main__":
    main()
