"""压缩文件类型图标集：白纸 + 右上折角 + 底部彩色格式色带。

为 Origami 关联的 9 种压缩格式各生成一套图标，导出 Windows .ico（多尺寸）与
macOS .icns（Pillow 跨平台写出，无需 iconutil）。母版用 Pillow 绘制（与 make_icon.py
同栈），8× 超采样后降采样到各尺寸，保证边缘平滑。

颜色是小尺寸下的第一区分维度（16px 时文字会糊，色带仍可辨）；格式文字是放大后的
第二维度。配色与设置里的格式色一致。

用法： py make_filetype_icons.py
输出： src-tauri/icons/filetypes/{ext}.ico / {ext}.icns，外加 preview.png 供肉眼校验。
"""
import os
from PIL import Image, ImageDraw, ImageFont

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, "src-tauri", "icons", "filetypes")
os.makedirs(OUT, exist_ok=True)

S = 8                     # 超采样倍率
M = 128 * S               # 母版边长（1024）
FONT_PATH = r"C:\Windows\Fonts\arialbd.ttf"

WHITE = (255, 255, 255, 255)
FLAP = (238, 241, 245, 255)      # 折角内侧浅灰
LINE = (232, 235, 241, 255)      # 文档内容横线
BORDER = (215, 219, 227, 255)    # 纸张描边

# [ext, 显示标签, 标签字号(128 设计坐标系), 色带颜色]
FORMATS = [
    ("zip", "ZIP", 17, (234, 106, 30)),
    ("7z",  "7Z",  18, (31, 157, 87)),
    ("rar", "RAR", 16, (124, 77, 219)),
    ("tar", "TAR", 16, (91, 102, 117)),
    ("gz",  "GZ",  18, (37, 150, 217)),
    ("tgz", "TGZ", 14, (17, 155, 142)),
    ("bz2", "BZ2", 15, (210, 59, 64)),
    ("xz",  "XZ",  18, (84, 87, 214)),
    ("zst", "ZST", 16, (217, 138, 18)),
]

# .ico 内嵌尺寸（资源管理器各视图）；.icns 由 Pillow 从母版生成标准金字塔。
ICO_SIZES = [16, 24, 32, 48, 64, 128, 256]


def px(v):
    return round(v * S)


def render_master(label, fs, color):
    """画一张 1024×1024 的 RGBA 母版。"""
    img = Image.new("RGBA", (M, M), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)

    # 文档外框（设计坐标 22,12 .. 106,116），右上折角，圆角 7。
    x0, y0, x1, y1 = 22, 12, 106, 116
    r = 7
    fx, fy = 84, 34          # 折痕：从 (84,12) 斜到 (106,34)
    sw = max(1, px(1.5))

    # 1) 白色纸张（四角圆角），随后裁掉右上被折掉的三角。
    d.rounded_rectangle([px(x0), px(y0), px(x1), px(y1)], radius=px(r),
                        fill=WHITE, outline=BORDER, width=sw)
    # 2) 裁掉折痕以上的右上角 → 透明。
    d.polygon([(px(84), px(12)), (px(x1), px(12)), (px(x1), px(fy))],
              fill=(0, 0, 0, 0))
    # 3) 折角内侧（折痕以下的小三角），浅灰。
    d.polygon([(px(84), px(12)), (px(x1), px(fy)), (px(84), px(fy))], fill=FLAP)
    # 4) 折痕 / 斜切边描边。
    d.line([(px(84), px(12)), (px(x1), px(fy))], fill=BORDER, width=sw)
    d.line([(px(84), px(fy)), (px(x1), px(fy))], fill=BORDER, width=sw)

    # 文档内容横线（折角下方）。
    for ly, lw in ((42, 52), (52, 60), (62, 40)):
        d.rounded_rectangle([px(34), px(ly), px(34 + lw), px(ly + 4)],
                            radius=px(2), fill=LINE)

    # 底部色带：仅下方两角圆角，与纸张底边吻合。
    band = Image.new("RGBA", (M, M), (0, 0, 0, 0))
    ImageDraw.Draw(band).rounded_rectangle(
        [px(x0), px(80), px(x1), px(y1)], radius=px(r),
        fill=color + (255,), corners=(False, False, True, True))
    img = Image.alpha_composite(img, band)

    # 格式文字（白色 Arial Bold，色带居中）。
    d = ImageDraw.Draw(img)
    font = ImageFont.truetype(FONT_PATH, px(fs))
    cx, cy = px((x0 + x1) / 2), px(98)
    d.text((cx, cy), label, font=font, fill=WHITE, anchor="mm")

    return img


def main():
    masters = []
    for ext, label, fs, color in FORMATS:
        master = render_master(label, fs, color)

        # .ico：用最大帧(256)作基图，Pillow 才会嵌入所有尺寸（基图小于某尺寸时该
        # 尺寸会被丢弃）。各帧预先 LANCZOS 降采样，质量优于让编码器内部缩放。
        frames = [master.resize((s, s), Image.LANCZOS) for s in ICO_SIZES]  # 升序
        base = frames[-1]  # 256×256
        base.save(os.path.join(OUT, f"{ext}.ico"), format="ICO",
                  sizes=[(s, s) for s in ICO_SIZES],
                  append_images=frames[:-1])

        # .icns：Pillow 从 1024 母版写出标准 icns（跨平台，无需 iconutil）。
        master.save(os.path.join(OUT, f"{ext}.icns"), format="ICNS")

        masters.append((ext, master))
        print(f"  {ext:<4} -> {ext}.ico  {ext}.icns")

    # 预览联系表（3×3，96px 摆在浅灰底上），仅供肉眼校验。
    cell, pad, cols = 120, 16, 3
    rows = (len(masters) + cols - 1) // cols
    pv = Image.new("RGB", (cols * cell, rows * cell), (245, 246, 248))
    for i, (ext, master) in enumerate(masters):
        ic = master.resize((96, 96), Image.LANCZOS)
        cxp = (i % cols) * cell + (cell - 96) // 2
        cyp = (i // cols) * cell + (cell - 96) // 2
        pv.paste(ic, (cxp, cyp), ic)
    pv.save(os.path.join(OUT, "preview.png"))
    print(f"\n已写出 {len(masters)} 套图标 + preview.png 到 {OUT}")


if __name__ == "__main__":
    main()
