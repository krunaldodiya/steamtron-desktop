const REPO = "krunaldodiya/SteamTron-Desktop";
const RELEASES = `https://github.com/${REPO}/releases`;
const API = `https://api.github.com/repos/${REPO}/releases/latest`;

function findAsset(assets, suffix) {
  return assets.find((asset) => String(asset.name || "").toLowerCase().endsWith(suffix));
}

function setLink(id, url) {
  for (const node of document.querySelectorAll(`#${id}`)) {
    if (url) {
      node.href = url;
      node.removeAttribute("aria-disabled");
    }
  }
}

async function loadLatestRelease() {
  const version = document.querySelector("#release-version");
  const note = document.querySelector("#release-note");
  try {
    const response = await fetch(API, {
      headers: { Accept: "application/vnd.github+json" },
    });
    if (!response.ok) throw new Error(`HTTP ${response.status}`);
    const release = await response.json();
    const assets = Array.isArray(release.assets) ? release.assets : [];
    const appimage = findAsset(assets, ".appimage");
    const deb = findAsset(assets, ".deb");
    const rpm = findAsset(assets, ".rpm");
    const setup = findAsset(assets, "setup.exe") || findAsset(assets, ".exe");
    const checksums = findAsset(assets, "sha256sums.txt");

    if (version) version.textContent = release.tag_name || "Latest release";
    if (note) note.textContent = release.published_at
      ? `Published ${new Date(release.published_at).toLocaleDateString(undefined, { year: "numeric", month: "short", day: "numeric" })}`
      : "Downloads are attached to GitHub Releases.";

    const linux = appimage || deb || rpm;
    if (linux) {
      setLink("download-linux", linux.browser_download_url);
      setLink("download-linux-bottom", linux.browser_download_url);
    }
    if (setup) {
      setLink("download-windows", setup.browser_download_url);
      setLink("download-windows-bottom", setup.browser_download_url);
    }
    if (appimage) {
      const link = document.querySelector('#package-links a[href*="releases"]');
      if (link) link.href = appimage.browser_download_url;
    }
    if (deb) {
      const links = document.querySelectorAll("#package-links a");
      if (links[1]) links[1].href = deb.browser_download_url;
    }
    if (rpm) {
      const links = document.querySelectorAll("#package-links a");
      if (links[2]) links[2].href = rpm.browser_download_url;
    }
    if (checksums) {
      const links = document.querySelectorAll("#package-links a");
      if (links[3]) links[3].href = checksums.browser_download_url;
    }
  } catch (error) {
    // The releases page remains the fallback when the repository is still
    // private or the API is rate-limited.
    if (version) version.textContent = "Latest release";
    if (note) {
      note.innerHTML = `<a href="${RELEASES}" target="_blank" rel="noreferrer">Open GitHub Releases</a> for downloads.`;
    }
  }
}

loadLatestRelease();
