"""Origami 压缩文件图标：C「折纸拉链」方案。

统一使用浅色折纸文件页与拉链结构，格式色同时作用于拉链和底部标签。16/24/32px
不是从大图直接缩放，而是分别减少折面、拉链齿、文字和鹤头细节，保证资源管理器与
Finder 小图标仍清楚。输出 Windows 多帧 ICO、包含现代小尺寸块的 macOS ICNS，
以及一个仅供肉眼检查、不会提交的 preview.png。

用法：python make_filetype_icons.py
输出：src-tauri/icons/filetypes/{ext}.ico / {ext}.icns
"""

from __future__ import annotations

import io
import os
import struct
from functools import lru_cache

from PIL import Image, ImageDraw, ImageFont


HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, "src-tauri", "icons", "filetypes")
os.makedirs(OUT, exist_ok=True)

# 128×128 设计坐标，始终在 8 倍画布绘制后降采样。
S = 8
M = 128 * S

WHITE = (255, 255, 255, 255)
PAPER_LEFT = (244, 247, 252, 255)
PAPER_RIGHT = (250, 251, 255, 255)
FLAP = (237, 242, 251, 255)
BORDER = (190, 199, 220, 255)
ZIPPER_DARK = (36, 44, 70, 255)
BEAK = (251, 191, 36, 255)

# 格式色均保持足够饱和度；形状相近时优先靠色块而不是小字区分。
# [扩展名, 标签, 128 设计坐标字号, RGB]
FORMATS = [
    ("zip", "ZIP", 17, (234, 106, 30)),
    ("7z", "7Z", 18, (31, 157, 87)),
    ("rar", "RAR", 16, (124, 77, 219)),
    ("tar", "TAR", 16, (91, 102, 117)),
    ("gz", "GZ", 18, (37, 150, 217)),
    ("tgz", "TGZ", 14, (17, 155, 142)),
    ("bz2", "BZ2", 15, (210, 59, 64)),
    ("xz", "XZ", 18, (84, 87, 214)),
    ("zst", "ZST", 16, (217, 138, 18)),
]

ICO_SIZES = [16, 24, 32, 48, 64, 128, 256]

# 现代 ICNS PNG 块。ic11/ic12 分别是 16/32 逻辑像素的 Retina 资源，
# 因而用更简洁的 profile 单独绘制，而不是复用相同物理尺寸的普通帧。
ICNS_SPECS = [
    (b"icp4", 16, "micro"),
    (b"icp5", 32, "compact"),
    (b"icp6", 64, "full"),
    (b"ic07", 128, "full"),
    (b"ic08", 256, "full"),
    (b"ic09", 512, "full"),
    (b"ic10", 1024, "full"),
    (b"ic11", 32, "micro"),
    (b"ic12", 64, "compact"),
    (b"ic13", 256, "full"),
    (b"ic14", 512, "full"),
]

FONT_CANDIDATES = [
    r"C:\Windows\Fonts\arialbd.ttf",
    "/System/Library/Fonts/Supplemental/Arial Bold.ttf",
    "/System/Library/Fonts/Helvetica.ttc",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
]


def px(value: float) -> int:
    return round(value * S)


@lru_cache(maxsize=None)
def font_at(size: int) -> ImageFont.FreeTypeFont | ImageFont.ImageFont:
    for path in FONT_CANDIDATES:
        if os.path.exists(path):
            return ImageFont.truetype(path, size)
    try:
        return ImageFont.truetype("DejaVuSans-Bold.ttf", size)
    except OSError:
        # Pillow 10+ 支持可缩放默认字体；旧版则回退到固定默认字体。
        try:
            return ImageFont.load_default(size=size)
        except TypeError:
            return ImageFont.load_default()


def make_page_mask() -> Image.Image:
    mask = Image.new("L", (M, M), 0)
    d = ImageDraw.Draw(mask)
    d.rounded_rectangle([px(18), px(8), px(107), px(123)], radius=px(7), fill=255)
    d.polygon([(px(82), px(8)), (px(107), px(8)), (px(107), px(33))], fill=0)
    return mask


def masked_fill(img: Image.Image, mask: Image.Image, color: tuple[int, int, int, int]) -> None:
    layer = Image.new("RGBA", img.size, color)
    img.alpha_composite(Image.composite(layer, Image.new("RGBA", img.size), mask))


def draw_page_outline(img: Image.Image) -> None:
    outline = Image.new("RGBA", img.size, (0, 0, 0, 0))
    d = ImageDraw.Draw(outline)
    width = px(1.7)
    d.rounded_rectangle(
        [px(18), px(8), px(107), px(123)],
        radius=px(7),
        outline=BORDER,
        width=width,
    )
    d.polygon(
        [(px(82), px(8)), (px(108), px(8)), (px(108), px(34))],
        fill=(0, 0, 0, 0),
    )
    d.line([(px(82), px(8)), (px(105), px(31))], fill=BORDER, width=width)
    d.line([(px(82), px(31)), (px(105), px(31))], fill=BORDER, width=width)
    img.alpha_composite(outline)


def default_profile(size: int) -> str:
    if size <= 16:
        return "micro"
    if size <= 24:
        return "small"
    if size <= 32:
        return "compact"
    return "full"


def render_frame(
    size: int,
    label: str,
    font_size: int,
    rgb: tuple[int, int, int],
    profile: str | None = None,
) -> Image.Image:
    """按目标逻辑尺寸绘制一帧；profile 可为 Retina 资源覆盖默认细节级别。"""
    detail = profile or default_profile(size)
    color = rgb + (255,)
    img = Image.new("RGBA", (M, M), (0, 0, 0, 0))
    page_mask = make_page_mask()
    masked_fill(img, page_mask, WHITE)

    # 48px 以上才保留折纸明暗面；32px 以下让轮廓更直接。
    if detail == "full":
        left = Image.new("RGBA", img.size, (0, 0, 0, 0))
        ImageDraw.Draw(left).polygon(
            [(px(18), px(8)), (px(61), px(8)), (px(61), px(88)), (px(18), px(116))],
            fill=PAPER_LEFT,
        )
        img.alpha_composite(Image.composite(left, Image.new("RGBA", img.size), page_mask))

        right = Image.new("RGBA", img.size, (0, 0, 0, 0))
        ImageDraw.Draw(right).polygon(
            [(px(61), px(8)), (px(82), px(8)), (px(105), px(31)), (px(105), px(91)), (px(61), px(76))],
            fill=PAPER_RIGHT,
        )
        img.alpha_composite(Image.composite(right, Image.new("RGBA", img.size), page_mask))

    d = ImageDraw.Draw(img)
    d.polygon([(px(82), px(8)), (px(105), px(31)), (px(82), px(31))], fill=FLAP)

    # 格式色底签先铺底，拉链头可以自然压在色带上。16/24px 不写字，只用大色块识别。
    band = Image.new("RGBA", img.size, (0, 0, 0, 0))
    ImageDraw.Draw(band).rectangle([px(18), px(91), px(107), px(124)], fill=color)
    img.alpha_composite(Image.composite(band, Image.new("RGBA", img.size), page_mask))

    # 拉链：越小越粗、齿越少，确保缩到 16px 后仍是清楚的像素块。
    if detail == "micro":
        rail_w, teeth = 10, [(51, 29, 64, 36), (64, 48, 77, 55), (51, 67, 64, 74)]
    elif detail == "small":
        rail_w, teeth = 8, [(51, 26, 64, 32), (64, 42, 77, 48), (51, 58, 64, 64), (64, 74, 77, 80)]
    else:
        rail_w = 5
        teeth = [(51, 24, 64, 29), (64, 35, 77, 40), (51, 46, 64, 51), (64, 57, 77, 62), (51, 68, 64, 73)]

    d.rounded_rectangle(
        [px(64 - rail_w / 2), px(18), px(64 + rail_w / 2), px(88)],
        radius=px(rail_w / 2),
        fill=color,
    )
    for x0, y0, x1, y1 in teeth:
        d.rounded_rectangle([px(x0), px(y0), px(x1), px(y1)], radius=px(2), fill=ZIPPER_DARK)

    if detail == "micro":
        d.rectangle([px(55), px(75), px(73), px(88)], fill=color)
    else:
        d.polygon([(px(52), px(78)), (px(72), px(78)), (px(68), px(94)), (px(56), px(94))], fill=color)
        if detail == "full":
            d.rounded_rectangle([px(58), px(81), px(66), px(88)], radius=px(2), fill=WHITE)
        # 黄色三角同时像拉链头和折纸鹤的喙；16px 时去掉，避免糊成杂点。
        d.polygon([(px(68), px(81)), (px(83), px(89)), (px(68), px(97))], fill=BEAK)

    if detail in {"compact", "full"}:
        d = ImageDraw.Draw(img)
        font = font_at(px(font_size))
        d.text((px(62.5), px(108)), label, font=font, fill=WHITE, anchor="mm")

    # 最后重描轮廓与折痕，避免彩色底签或折面污染边缘。
    draw_page_outline(img)
    if size == M:
        return img
    return img.resize((size, size), Image.Resampling.LANCZOS)


def png_bytes(image: Image.Image) -> bytes:
    stream = io.BytesIO()
    image.save(stream, format="PNG", optimize=True)
    return stream.getvalue()


def save_icns(
    path: str,
    label: str,
    font_size: int,
    color: tuple[int, int, int],
) -> None:
    blocks: list[tuple[bytes, bytes]] = []
    for code, size, profile in ICNS_SPECS:
        blocks.append((code, png_bytes(render_frame(size, label, font_size, color, profile))))

    toc_payload = b"".join(code + struct.pack(">I", 8 + len(payload)) for code, payload in blocks)
    toc = b"TOC " + struct.pack(">I", 8 + len(toc_payload)) + toc_payload
    resources = b"".join(code + struct.pack(">I", 8 + len(payload)) + payload for code, payload in blocks)
    body = toc + resources
    with open(path, "wb") as fp:
        fp.write(b"icns")
        fp.write(struct.pack(">I", 8 + len(body)))
        fp.write(body)


def save_preview(masters: list[tuple[str, str, int, tuple[int, int, int]]]) -> None:
    cell_w, cell_h, cols = 280, 150, 3
    rows = (len(masters) + cols - 1) // cols
    preview = Image.new("RGB", (cols * cell_w, rows * cell_h), (244, 247, 252))
    d = ImageDraw.Draw(preview)
    title_font = font_at(18)
    size_font = font_at(11)

    for i, (ext, label, font_size, color) in enumerate(masters):
        x = (i % cols) * cell_w
        y = (i // cols) * cell_h
        d.text((x + 16, y + 12), label, font=title_font, fill=(24, 32, 51))

        large = render_frame(96, label, font_size, color)
        preview.paste(large, (x + 16, y + 39), large)

        sx = x + 145
        for j, size in enumerate((32, 24, 16)):
            frame = render_frame(size, label, font_size, color)
            sy = y + 40 + j * 35
            preview.paste(frame, (sx, sy), frame)
            d.text((sx + 42, sy + 5), f"{size}px", font=size_font, fill=(91, 102, 117))

    preview.save(os.path.join(OUT, "preview.png"))


def icns_codes(path: str) -> list[bytes]:
    with open(path, "rb") as fp:
        data = fp.read()
    if data[:4] != b"icns" or len(data) < 8:
        raise ValueError(f"无效 ICNS：{path}")
    declared = struct.unpack(">I", data[4:8])[0]
    if declared != len(data):
        raise ValueError(f"ICNS 长度不匹配：{path}")

    codes: list[bytes] = []
    offset = 8
    while offset < len(data):
        if offset + 8 > len(data):
            raise ValueError(f"ICNS 块头不完整：{path}")
        code = data[offset : offset + 4]
        length = struct.unpack(">I", data[offset + 4 : offset + 8])[0]
        if length < 8 or offset + length > len(data):
            raise ValueError(f"ICNS 块长度无效：{path} / {code!r}")
        codes.append(code)
        offset += length
    return codes


def validate_outputs() -> None:
    expected_ico = {(size, size) for size in ICO_SIZES}
    expected_icns = {code for code, _, _ in ICNS_SPECS}
    for ext, _, _, _ in FORMATS:
        ico_path = os.path.join(OUT, f"{ext}.ico")
        with Image.open(ico_path) as ico:
            actual = set(ico.ico.sizes())
            if actual != expected_ico:
                raise ValueError(f"{ext}.ico 尺寸不完整：{sorted(actual)}")
            ico.load()

        icns_path = os.path.join(OUT, f"{ext}.icns")
        actual_codes = set(icns_codes(icns_path))
        missing = expected_icns - actual_codes
        if missing:
            raise ValueError(f"{ext}.icns 缺少资源块：{sorted(missing)}")
        with Image.open(icns_path) as icns:
            icns.load()

    print("已校验 ICO 多尺寸帧与 ICNS 小尺寸/Retina 资源块")


def main() -> None:
    for ext, label, font_size, color in FORMATS:
        frames = {size: render_frame(size, label, font_size, color) for size in ICO_SIZES}
        base = frames[256]
        base.save(
            os.path.join(OUT, f"{ext}.ico"),
            format="ICO",
            sizes=[(size, size) for size in ICO_SIZES],
            append_images=[frames[size] for size in ICO_SIZES if size != 256],
        )
        save_icns(os.path.join(OUT, f"{ext}.icns"), label, font_size, color)
        print(f"  {ext:<4} -> {ext}.ico  {ext}.icns")

    save_preview(FORMATS)
    validate_outputs()
    print(f"\n已写出 {len(FORMATS)} 套 C 方案图标与 preview.png 到 {OUT}")


if __name__ == "__main__":
    main()
