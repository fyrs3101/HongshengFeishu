#!/usr/bin/env python3
# 生成鸿升飞书 Tauri 图标集：32/128/128@2x png + icns + ico
import math
import os
from PIL import Image, ImageDraw

OUT = os.path.join(os.path.dirname(__file__), "..", "src-tauri", "icons")
os.makedirs(OUT, exist_ok=True)


def make_icon(size):
    img = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    # 圆角渐变底
    r = size * 0.22
    d.rounded_rectangle([0, 0, size, size], radius=r, fill=(91, 108, 255, 255))
    # 叠加紫色
    d.rounded_rectangle(
        [size * 0.35, size * 0.35, size, size], radius=r, fill=(154, 91, 255, 200)
    )
    # 纸飞机
    cx, cy = size / 2, size / 2
    s = size * 0.30
    white = (255, 255, 255, 255)
    wing = (235, 238, 255, 255)
    body = [
        (cx + s * 0.95, cy - s * 0.55),
        (cx - s * 1.05, cy - s * 0.15),
        (cx + s * 0.05, cy + s * 0.18),
        (cx - s * 1.05, cy + s * 0.55),
        (cx + s * 0.95, cy - s * 0.55),
    ]
    d.polygon(body, fill=white)
    fold = [
        (cx + s * 0.95, cy - s * 0.55),
        (cx - s * 1.05, cy - s * 0.15),
        (cx + s * 0.05, cy + s * 0.18),
    ]
    d.polygon(fold, fill=wing)
    return img


base = make_icon(1024)
base.save(os.path.join(OUT, "32x32.png"), size=(32, 32)) if False else None
make_icon(32).save(os.path.join(OUT, "32x32.png"))
make_icon(128).save(os.path.join(OUT, "128x128.png"))
make_icon(256).save(os.path.join(OUT, "128x128@2x.png"))

# ico 多尺寸
make_icon(256).save(os.path.join(OUT, "icon.ico"), sizes=[(16, 16), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)])

# icns 多尺寸
make_icon(1024).save(
    os.path.join(OUT, "icon.icns"),
    sizes=[(16, 16), (32, 32), (64, 64), (128, 128), (256, 256), (512, 512), (1024, 1024)],
)

print("icons generated:", sorted(os.listdir(OUT)))
