# Software preview of the engine's quads (Horizon mob frame: blocks, +z the
# way the mob faces, +x its left, y up). usage: raster.py quads.json out.png assets_root [yaw_deg]
import json, sys, math
import numpy as np
from PIL import Image

d = json.load(open(sys.argv[1])); out = sys.argv[2]; root = sys.argv[3]
fp = len(sys.argv) > 4 and sys.argv[4] == 'fp'
yaw = 0 if fp else math.radians(float(sys.argv[4]) if len(sys.argv) > 4 else 30)
W = H = 480
img = np.zeros((H, W, 3), np.float32); img[:] = (0.55, 0.7, 0.9)
zb = np.full((H, W), -1e9, np.float32)
cache = {}
def tex(sprite):
    if sprite in cache: return cache[sprite]
    ns, path = sprite.split(':', 1) if ':' in sprite else ('minecraft', sprite)
    try: t = np.asarray(Image.open(f"{root}/assets/{ns}/textures/{path}.png").convert('RGBA'), np.float32) / 255
    except Exception: t = None
    cache[sprite] = t; return t
# camera: look at the mob from the front-left, slightly above
cy, sy = math.cos(yaw), math.sin(yaw)
pitch = math.radians(15); cp, sp = math.cos(pitch), math.sin(pitch)
def view(p):
    if fp:
        x, y, z = p[0], p[1] - 1.62, p[2]
        f = (H / 2) / math.tan(math.radians(35))
        d = max(-z, 1e-3)
        return (W / 2 + x * f / d, H / 2 - y * f / d, z)
    x, y, z = p
    # turn the mob so its front (+z) faces the camera (camera at +z), then yaw
    X = x * cy + z * sy; Z = -x * sy + z * cy
    Y = y * cp - Z * sp; Z2 = y * sp + Z * cp
    s = 150
    return (W / 2 + X * s, H * 0.85 - Y * s, Z2)   # the camera faces the mob: its left (+x) is on the viewer's right
light = np.array([0.3, 0.8, 0.5]); light /= np.linalg.norm(light)
for q in d['quads']:
    t = tex(q['s'])
    c = [np.array(v) for v in q['c']]
    n = np.cross(c[1] - c[0], c[2] - c[1]); nl = np.linalg.norm(n)
    shade = 0.55 + 0.45 * max(0, float(np.dot(n / nl, light))) if nl > 0 else 1
    u0, v0, u1, v1 = [x / 16 for x in q['uv']]
    uvs = [(u0, v0), (u0, v1), (u1, v1), (u1, v0)]
    P = [view(v) for v in q['c']]
    # back-face cull in screen space: the order must be counter-clockwise as seen (y down => clockwise)
    area = sum(P[i][0] * P[(i + 1) % 4][1] - P[(i + 1) % 4][0] * P[i][1] for i in range(4))
    if area > 0: continue
    for tri in ((0, 1, 2), (0, 2, 3)):
        (x0, y0, z0), (x1, y1, z1), (x2, y2, z2) = [P[i] for i in tri]
        a0, a1, a2 = [uvs[i] for i in tri]
        mnx, mxx = int(max(0, min(x0, x1, x2))), int(min(W - 1, max(x0, x1, x2)) + 1)
        mny, mxy = int(max(0, min(y0, y1, y2))), int(min(H - 1, max(y0, y1, y2)) + 1)
        den = (y1 - y2) * (x0 - x2) + (x2 - x1) * (y0 - y2)
        if abs(den) < 1e-9 or mnx >= mxx or mny >= mxy: continue
        xs, ys = np.meshgrid(np.arange(mnx, mxx) + 0.5, np.arange(mny, mxy) + 0.5)
        w0 = ((y1 - y2) * (xs - x2) + (x2 - x1) * (ys - y2)) / den
        w1 = ((y2 - y0) * (xs - x2) + (x0 - x2) * (ys - y2)) / den
        w2 = 1 - w0 - w1
        m = (w0 >= 0) & (w1 >= 0) & (w2 >= 0)
        z = w0 * z0 + w1 * z1 + w2 * z2
        u = w0 * a0[0] + w1 * a1[0] + w2 * a2[0]; v = w0 * a0[1] + w1 * a1[1] + w2 * a2[1]
        if t is not None:
            th, tw = t.shape[:2]
            px = t[np.clip((v * th).astype(int), 0, th - 1), np.clip((u * tw).astype(int), 0, tw - 1)]
            m &= px[..., 3] > 0.1
            col = px[..., :3]
        else:
            col = np.zeros(xs.shape + (3,)); col[:] = (1, 0, 1)
        sub = zb[mny:mxy, mnx:mxx]; m &= z > sub
        tint = q['t']; tc = np.array([(tint >> 16) & 255, (tint >> 8) & 255, tint & 255]) / 255
        img[mny:mxy, mnx:mxx][m] = (col * tc * (1 if q['fb'] else shade))[m]
        sub[m] = z[m]
Image.fromarray((np.clip(img, 0, 1) * 255).astype(np.uint8)).resize((W, H), Image.NEAREST).save(out)
print('ok', out, len(d['quads']))
