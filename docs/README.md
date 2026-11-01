# Docs

**Social preview.** In the repository settings (General, Social preview) upload [`social-preview.png`](social-preview.png) (1280x640). The wide banner at the top of the README is [`banner.png`](banner.png) (1600x800).

| File | What it is | Rebuild |
| --- | --- | --- |
| `banner.png` | README banner: pitch black, one moonlit rim | `node docs/banner-source/shot.js` (Playwright; Inter font; `assets/brand/png/allblack11-512.png`) |
| `hero.png`, `bento.png`, `windows-gallery.png`, `social-preview.png` | Mockups of the real app, styled for the README | `python3 docs/mockup-source/make_mockups.py` (numpy, Pillow) |
| `mockup-source/shots/` | The window-only screenshots the mockups use, from a Windows 11 ARM64 PC, with pop-ups cropped or blacked out and nothing personal | taken by hand; the raw captures are not kept |
| `screenshots/` | Other screenshots; see [its README](screenshots/README.md) | |

The long configuration reference is [`CONFIG.md`](CONFIG.md). Development notes (evidence, inventories, release policy, smoke test) are in [`dev`](dev).
