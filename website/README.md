# Website

The project's landing page, published to GitHub Pages by
[`.github/workflows/website.yml`](../.github/workflows/website.yml) on every
push to `main` that touches it. It's plain HTML and CSS with no build step.

Its look follows the app: the same greys, accent, radius and fonts (see
`apps/desktop/src/ui/style.rs`), a title bar of bordered tabs, and the
status bar's indicator dots. The layout (a narrow column of icon-led groups)
follows [razza.io](https://razza.io/).

The workflow adds `logo.svg` from the repository root, the Nunito Sans fonts
from `apps/desktop/assets/fonts/` and the icons from
`lib/icons/assets/`, so those aren't duplicated here. Icons are drawn as CSS
masks (`<span class="icon" style="--icon: url(icons/…)">`), so they take the
text colour like they do in the app.

Console text is set in [Commit Mono](https://commitmono.com/) 1.143, which
only the website uses, so it lives here in `fonts/` with its licence (SIL
OFL 1.1). To preview locally, stage the site the
same way:

```sh
mkdir -p _site/fonts _site/icons
cp -r website/. _site/
cp logo.svg _site/
cp lib/icons/assets/*.svg _site/icons/
cp apps/desktop/assets/fonts/NunitoSans-{Medium,Bold,SemiExpandedBlack}.ttf _site/fonts/
python -m http.server -d _site
```

The screenshots are the app's `website` and `website-light` review
snapshots, which show the containerdesk mock's desktop as the remote frame.
Render them headlessly:

```sh
SNAPSHOT_DIR=/some/dir cargo test -p server-remote render_review_snapshots -- --ignored
```

then copy `website-wgpu.png` and `website-light-wgpu.png` over
`screenshot-dark.png` and `screenshot-light.png`.

Keep the protocol list and status in step with the README when a protocol's
status changes.
