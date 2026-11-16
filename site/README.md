# SteamTron landing page

Static landing page for `steamtron.vercel.app`.

## Local preview

```sh
python3 -m http.server 4173 --directory site
# open http://127.0.0.1:4173
```

## Deploy to Vercel

The page is plain HTML/CSS/JS, so it can be deployed as a static Vercel project
without a framework build.

```sh
cd site
vercel
vercel --prod
```

In the Vercel project settings:

- Framework preset: **Other**
- Root directory: `site` (or deploy with `site` as the current directory)
- Build command: empty
- Output directory: `.` (the repository’s `site/` folder)

Assign the project domain `steamtron.vercel.app` after the first deployment.

## Downloads

`downloads.js` reads the latest public release from GitHub and populates the
Linux AppImage, Windows installer, deb, rpm, and checksum links. If the
repository is private or GitHub API is rate-limited, every button falls back to
the GitHub Releases page.

## Content

The page intentionally avoids fake screenshots and game artwork. The app
mockup is built from CSS shapes. Update the repository URLs and release source
if the desktop repository is renamed.
