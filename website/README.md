# Website

The project's landing page, published to GitHub Pages by
[`.github/workflows/website.yml`](../.github/workflows/website.yml) on every
push to `main` that touches it. It's plain HTML and CSS with no build step.

The workflow adds `logo.svg` from the repository root and the Nunito Sans
fonts from `apps/server-remote/assets/fonts/`, so those aren't duplicated
here. To preview locally with them, stage the site the same way:

```sh
mkdir -p _site/fonts
cp -r website/. _site/
cp logo.svg _site/
cp apps/server-remote/assets/fonts/NunitoSans-{Medium,Bold,SemiExpandedBlack}.ttf _site/fonts/
python -m http.server -d _site
```

The screenshots are the app's own `power-menu` review snapshots (dark and
light), rendered headlessly:

```sh
SNAPSHOT_DIR=/some/dir cargo test -p server-remote render_review_snapshots -- --ignored
```

then copy `power-menu-wgpu.png` and `power-menu-light-wgpu.png` over
`screenshot-dark.png` and `screenshot-light.png`.

Keep the protocol table and status in step with the README when a protocol's
status changes.
