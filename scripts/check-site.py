#!/usr/bin/env python3
"""Checks the static website in site/ (stdlib only; run from anywhere).

Fails (exit 1) when:
  * a local link, image, icon or stylesheet points at a file that is not in site/
  * a #fragment link has no matching id
  * an <img> has no alt attribute
  * the page loads anything from another host (stylesheet, script, image, font): the page promises
    "no third-party requests"
  * a download link does not use the tag that matches the app version in Cargo.toml
    (so a version bump cannot leave the site pointing at an older release by accident)
  * canonical, og:url and og:image disagree about the site address
"""
import re
import sys
from html.parser import HTMLParser
from pathlib import Path
from urllib.parse import urlparse

ROOT = Path(__file__).resolve().parent.parent
SITE = ROOT / "site"
REPO = "https://github.com/TH0MSX/AllBlack11"
DOWNLOADS = {
    "AllBlack11-Setup-arm64.exe",
    "AllBlack11-arm64.zip",
    "AllBlack11-Setup-x64.exe",
    "AllBlack11-x64.zip",
}


class Page(HTMLParser):
    def __init__(self):
        super().__init__()
        self.links = []  # (tag, attr, value)
        self.ids = set()
        self.imgs_without_alt = []
        self.meta = {}
        self.canonical = None

    def handle_starttag(self, tag, attrs):
        a = dict(attrs)
        if "id" in a:
            self.ids.add(a["id"])
        if tag == "img":
            if "alt" not in a:
                self.imgs_without_alt.append(a.get("src", "?"))
            if "src" in a:
                self.links.append((tag, "src", a["src"]))
        elif tag == "a" and "href" in a:
            self.links.append((tag, "href", a["href"]))
        elif tag == "link" and "href" in a:
            rel = a.get("rel", "")
            if rel == "canonical":
                self.canonical = a["href"]
            else:
                self.links.append((tag, "href", a["href"]))
        elif tag in ("script", "source", "iframe", "video", "audio") and "src" in a:
            self.links.append((tag, "src", a["src"]))
        elif tag == "meta":
            key = a.get("property") or a.get("name")
            if key:
                self.meta[key] = a.get("content", "")


def cargo_version():
    text = (ROOT / "Cargo.toml").read_text(encoding="utf-8")
    m = re.search(r'^version\s*=\s*"([^"]+)"', text, re.M)
    if not m:
        sys.exit("could not read the version from Cargo.toml")
    return m.group(1)


def check_page(path, errors, tag):
    page = Page()
    page.feed(path.read_text(encoding="utf-8"))
    name = path.name

    for src in page.imgs_without_alt:
        errors.append(f"{name}: <img src={src}> has no alt attribute")

    for tag_name, attr, value in page.links:
        if value.startswith("#"):
            if len(value) > 1 and value[1:] not in page.ids:
                errors.append(f"{name}: #{value[1:]} has no matching id")
            continue
        if value.startswith(("mailto:", "tel:")):
            continue
        url = urlparse(value)
        if url.scheme in ("http", "https"):
            # Plain outbound links are fine. Loading a resource from another host is not.
            if tag_name != "a":
                errors.append(f"{name}: <{tag_name} {attr}={value}> loads a third-party resource")
            m = re.match(re.escape(REPO) + r"/releases/download/([^/]+)/([^/?#]+)$", value)
            if m:
                if m.group(1) != tag:
                    errors.append(f"{name}: download link uses {m.group(1)}, app version is {tag}: {value}")
                if m.group(2) not in DOWNLOADS:
                    errors.append(f"{name}: unknown download file {m.group(2)}")
            continue
        if url.scheme:
            errors.append(f"{name}: unsupported link scheme in {value}")
            continue
        target = (path.parent / url.path).resolve()
        if not str(target).startswith(str(SITE.resolve())):
            errors.append(f"{name}: {value} leaves site/")
        elif not target.is_file():
            errors.append(f"{name}: {value} does not exist in site/")
    return page


def main():
    errors = []
    version = cargo_version()
    tag = f"v{version}"

    index = SITE / "index.html"
    notfound = SITE / "404.html"
    for need in (index, notfound, SITE / "style.css", SITE / "robots.txt"):
        if not need.is_file():
            errors.append(f"missing {need.relative_to(ROOT)}")
    if errors:
        print("\n".join(errors))
        return 1

    page = check_page(index, errors, tag)
    check_page(notfound, errors, tag)
    # The 404 page cannot use relative links, so it carries the site address; it must be the same one.
    if page.canonical and page.canonical not in notfound.read_text(encoding="utf-8"):
        errors.append(f"404.html: does not link back to {page.canonical}")

    # Every expected download must be present exactly as linked (4 files, all on this tag).
    html = index.read_text(encoding="utf-8")
    found = set(re.findall(re.escape(REPO) + r"/releases/download/" + re.escape(tag) + r"/([^\"'<>\s]+)", html))
    for missing in sorted(DOWNLOADS - found):
        errors.append(f"index.html: no download link for {missing} on {tag}")

    # One site address everywhere.
    canon = page.canonical
    ogurl = page.meta.get("og:url")
    ogimg = page.meta.get("og:image", "")
    if not canon:
        errors.append("index.html: missing canonical link")
    else:
        if ogurl != canon:
            errors.append(f"index.html: og:url {ogurl} != canonical {canon}")
        if not ogimg.startswith(canon):
            errors.append(f"index.html: og:image {ogimg} is not under {canon}")
        rel = ogimg[len(canon):] if ogimg.startswith(canon) else ""
        if rel and not (SITE / rel).is_file():
            errors.append(f"index.html: og:image file {rel} is not in site/")

    if errors:
        print("site check FAILED:")
        for e in errors:
            print("  -", e)
        return 1
    print(f"site check ok: {len(page.links)} links, {len(page.ids)} ids, downloads on {tag}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
