# Renders the frames `film` wrote (one quads file each) to PNGs and a GIF.
# usage: film.py <frames dir> <assets root> <tp|fp> <out.gif> [yaw_deg] [size]
# Same projection as raster.py: third person looks at the model from the
# front-left; first person is the eye at 1.62 looking down -z, with the arm
# offset the glue applies in game (fp_arm_offset default: -0.12, +0.2; FP_OFF overrides). Textures the
# preview cannot have (vanilla's own items and blocks) show as a grey checker.
import json, sys, math, glob, os
import numpy as np
from PIL import Image, ImageDraw

frames_dir, root, mode, out = sys.argv[1:5]
yaw = math.radians(float(sys.argv[5])) if len(sys.argv) > 5 else math.radians(30)
W = H = int(sys.argv[6]) if len(sys.argv) > 6 else 360
fp = mode == 'fp'
FP_OFF = tuple(float(x) for x in os.environ.get("FP_OFF", "-0.12,0.2,0").split(","))
cache = {}
checker = np.ones((16, 16, 4), np.float32) * 0.62
checker[(np.indices((16, 16)).sum(0) // 4) % 2 == 0, :3] = 0.5
checker[..., 3] = 1


def tex(sprite):
    if sprite in cache:
        return cache[sprite]
    ns, path = sprite.split(':', 1) if ':' in sprite else ('minecraft', sprite)
    try:
        t = np.asarray(Image.open(f"{root}/assets/{ns}/textures/{path}.png").convert('RGBA'), np.float32) / 255
    except Exception:
        t = checker
    cache[sprite] = t
    return t


cy, sy = math.cos(yaw), math.sin(yaw)
pitch = math.radians(12)
cp, sp = math.cos(pitch), math.sin(pitch)
scale = H / 3.0


def view(p):
    if fp:
        x, y, z = p[0] + FP_OFF[0], p[1] + FP_OFF[1] - 1.62, p[2] + FP_OFF[2]
        f = (H / 2) / math.tan(math.radians(35))
        d = max(-z, 1e-3)
        return (W / 2 + x * f / d, H / 2 - y * f / d, z)
    x, y, z = p
    X = x * cy + z * sy
    Z = -x * sy + z * cy
    Y = (y - 1.0) * cp - Z * sp
    Z2 = (y - 1.0) * sp + Z * cp
    return (W / 2 + X * scale, H * 0.52 - Y * scale, Z2)


light = np.array([0.3, 0.8, 0.5])
light /= np.linalg.norm(light)


def render(d):
    img = np.zeros((H, W, 3), np.float32)
    g = np.linspace(0, 1, H)[:, None]
    img[:] = np.stack([0.50 + 0.2 * g, 0.66 + 0.15 * g, 0.92 - 0.1 * g], -1)
    if not fp:
        img[int(H * 0.52 + 1.0 * cp * scale):, :] = (0.36, 0.55, 0.30)
    zb = np.full((H, W), -1e9, np.float32)
    for q in d['quads']:
        t = tex(q['s'])
        c = [np.array(v) for v in q['c']]
        n = np.cross(c[1] - c[0], c[2] - c[1])
        nl = np.linalg.norm(n)
        shade = 0.55 + 0.45 * max(0, float(np.dot(n / nl, light))) if nl > 0 else 1
        u0, v0, u1, v1 = [x / 16 for x in q['uv']]
        uvs = [(u0, v0), (u0, v1), (u1, v1), (u1, v0)]
        P = [view(v) for v in q['c']]
        area = sum(P[i][0] * P[(i + 1) % 4][1] - P[(i + 1) % 4][0] * P[i][1] for i in range(4))
        if area > 0:
            continue
        for tri in ((0, 1, 2), (0, 2, 3)):
            (x0, y0, z0), (x1, y1, z1), (x2, y2, z2) = [P[i] for i in tri]
            a0, a1, a2 = [uvs[i] for i in tri]
            mnx, mxx = int(max(0, min(x0, x1, x2))), int(min(W - 1, max(x0, x1, x2)) + 1)
            mny, mxy = int(max(0, min(y0, y1, y2))), int(min(H - 1, max(y0, y1, y2)) + 1)
            den = (y1 - y2) * (x0 - x2) + (x2 - x1) * (y0 - y2)
            if abs(den) < 1e-9 or mnx >= mxx or mny >= mxy:
                continue
            xs, ys = np.meshgrid(np.arange(mnx, mxx) + 0.5, np.arange(mny, mxy) + 0.5)
            w0 = ((y1 - y2) * (xs - x2) + (x2 - x1) * (ys - y2)) / den
            w1 = ((y2 - y0) * (xs - x2) + (x0 - x2) * (ys - y2)) / den
            w2 = 1 - w0 - w1
            m = (w0 >= 0) & (w1 >= 0) & (w2 >= 0)
            z = w0 * z0 + w1 * z1 + w2 * z2
            u = w0 * a0[0] + w1 * a1[0] + w2 * a2[0]
            v = w0 * a0[1] + w1 * a1[1] + w2 * a2[1]
            th, tw = t.shape[:2]
            px = t[np.clip((v * th).astype(int), 0, th - 1), np.clip((u * tw).astype(int), 0, tw - 1)]
            m &= px[..., 3] > 0.1
            col = px[..., :3]
            sub = zb[mny:mxy, mnx:mxx]
            m &= z > sub
            tint = q['t']
            tc = np.array([(tint >> 16) & 255, (tint >> 8) & 255, tint & 255]) / 255
            img[mny:mxy, mnx:mxx][m] = (col * tc * (1 if q['fb'] else shade))[m]
            sub[m] = z[m]
    return Image.fromarray((np.clip(img, 0, 1) * 255).astype(np.uint8))


files = sorted(glob.glob(os.path.join(frames_dir, '*.json')))
ims = [render(json.load(open(f))) for f in files]
label = os.path.splitext(os.path.basename(out))[0]
for im in ims:
    ImageDraw.Draw(im).text((6, 4), label, fill=(255, 255, 255))
ims[0].save(out, save_all=True, append_images=ims[1:], duration=50, loop=0, optimize=True)
ims[len(ims) // 2].save(out[:-4] + '.png')
print('ok', out, len(ims))
