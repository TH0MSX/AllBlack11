# Website

The project site lives in [`site/`](../../site): plain HTML and one CSS file, no framework, no build step,
no JavaScript, no web fonts, no analytics, no third-party requests (`scripts/check-site.py` enforces the
last one). Images are WebP copies of the README images in `docs/` (`hero`, `bento`, `windows-gallery`),
the social card is `social-preview.png`, the icon is `assets/brand/allblack11-icon.svg`.

## Hosting: GitHub Pages (free, no ads)

`.github/workflows/pages.yml` publishes `site/` on every push to `main` that touches it (and by hand from
Actions > Pages > Run workflow). GitHub's own limits for Pages: a published site up to 1 GB, a soft
100 GB per month of bandwidth, deployments time out after 10 minutes, and not for running an online
business or e-commerce (a tip link is not that). The site is about 0.6 MB.

**One-time owner step:** Settings > Pages > Build and deployment > Source: **GitHub Actions**. Until that is
set the `configure-pages` step fails. After it, the address is `https://th0msx.github.io/AllBlack11/`.

## Free custom domain: is-a.dev

Checked on 2026-11-01: `allblack11.is-a.dev` redirects to is-a.dev's "available" page. Its rules
(docs.is-a.dev/quickstart): a personal website or a non-commercial software project, and the site must
already be reasonably complete, so publish on Pages first. Registration is a pull request that adds one
file to a fork of `is-a-dev/register`:

`domains/allblack11.json`

```json
{
    "owner": {
        "username": "TH0MSX"
    },
    "records": {
        "CNAME": "th0msx.github.io"
    }
}
```

The register repository says AI-written requests are rejected, so write the pull request description
yourself, fill the template in as it is, and include the site link and a screenshot. After it is merged:

1. Settings > Pages > Custom domain: `allblack11.is-a.dev` (only after the merge), then tick **Enforce HTTPS**.
2. Change the site address in three places, then run `python3 scripts/check-site.py`:
   `site/index.html` (`canonical`, `og:url`, `og:image`) and the link in `site/404.html`. All other links
   are relative, so nothing else moves.

Other free options that were looked at (DigitalPlat FreeDomain suffixes, `<org>.github.io`, host-provided
subdomains) are weaker for a site that offers an unsigned installer; a paid `.com` is the only clean
alternative.

## Keeping it true

* **Releases.** The download links name the release tag. `check-site.py` compares it with the version in
  `Cargo.toml` and fails CI if they differ, so a version bump forces a site update (all four links in
  `site/index.html`, plus the "Download 3.8 Preview" wording).
* **Claims.** The status table and the safety table mirror the README ("How far it has been checked",
  "Safety") and `docs/dev/EVIDENCE.md`. Change them together.
* **Checks.** `python3 scripts/check-site.py` verifies local files, fragment links, image alt text,
  no third-party resources, the download tag and one consistent site address.
