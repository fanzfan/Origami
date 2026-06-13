"""Origami app icon: paper crane at dusk. Renders app-icon.png (1024x1024)."""
from PIL import Image, ImageDraw, ImageFilter

S = 4  # supersample
W = 1024 * S

def P(pts):
    return [(x * S, y * S) for x, y in pts]

# ---- background: diagonal dusk gradient inside rounded rect ----
grad = Image.linear_gradient("L").resize((W * 2, W * 2)).rotate(45, expand=False)
left = grad.size[0] // 4
grad = grad.crop((left, left, left + W, left + W))
top = Image.new("RGB", (W, W), (124, 92, 246))      # violet dusk
bottom = Image.new("RGB", (W, W), (45, 165, 240))   # sky cyan
bg = Image.composite(bottom, top, grad)

# subtle vignette glow behind crane
glow = Image.new("L", (W, W), 0)
ImageDraw.Draw(glow).ellipse(P([(180, 240), (900, 880)]), fill=70)
glow = glow.filter(ImageFilter.GaussianBlur(60 * S))
bg = Image.composite(Image.new("RGB", (W, W), (255, 255, 255)), bg, glow)

img = bg.convert("RGBA")
d = ImageDraw.Draw(img)

# ---- moon ----
d.ellipse(P([(742, 128), (878, 264)]), fill=(255, 255, 255, 110))
d.ellipse(P([(758, 144), (862, 248)]), fill=(255, 255, 255, 90))

# ---- soft shadow under crane ----
sh = Image.new("RGBA", (W, W), (0, 0, 0, 0))
ImageDraw.Draw(sh).ellipse(P([(250, 770), (790, 850)]), fill=(20, 20, 80, 70))
sh = sh.filter(ImageFilter.GaussianBlur(18 * S))
img = Image.alpha_composite(img, sh)
d = ImageDraw.Draw(img)

# ---- crane facets (paper whites, light from upper-left) ----
WHITE = (255, 255, 255, 255)
PAPER1 = (244, 247, 252, 255)
PAPER2 = (228, 234, 246, 255)
PAPER3 = (208, 217, 238, 255)

# tail spike
d.polygon(P([(128, 452), (300, 618), (420, 688)]), fill=PAPER3)
# body (two facets split along ridge)
d.polygon(P([(280, 622), (530, 516), (492, 768)]), fill=PAPER2)
d.polygon(P([(530, 516), (712, 622), (492, 768)]), fill=PAPER1)
# wing (large, two facets)
d.polygon(P([(352, 598), (468, 168), (560, 545)]), fill=WHITE)
d.polygon(P([(468, 168), (560, 545), (652, 566)]), fill=PAPER1)
# neck + head
d.polygon(P([(648, 580), (852, 348), (736, 656)]), fill=PAPER2)
d.polygon(P([(852, 348), (824, 432), (736, 656)]), fill=PAPER1)
# head & beak
d.polygon(P([(852, 348), (922, 386), (842, 426)]), fill=(251, 191, 36, 255))

img = img.convert("RGB")

# ---- rounded-rect mask with macOS margins (content 824/1024) ----
FULL = 1024 * S
canvas = Image.new("RGBA", (FULL, FULL), (0, 0, 0, 0))
content = img.resize((824 * S, 824 * S), Image.LANCZOS)
mask = Image.new("L", content.size, 0)
ImageDraw.Draw(mask).rounded_rectangle(
    [(0, 0), (content.size[0] - 1, content.size[1] - 1)],
    radius=185 * S * 824 // 1024,
    fill=255,
)
canvas.paste(content, (100 * S, 100 * S), mask)
canvas = canvas.resize((1024, 1024), Image.LANCZOS)
canvas.save("app-icon.png")
print("saved app-icon.png")
