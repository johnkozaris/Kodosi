#!/usr/bin/env python3
"""Writes the static update site of one release, and SHA256SUMS of its files.

The release folder holds the files of one version: Kodosi-X.Y.Z.dmg with its Sparkle
deltas, kodosi_X.Y.Z_amd64.deb and the other Linux files. The site gets the Sparkle feed
from generate-appcast.sh, the apt repository, release.json and _redirects. The packages
stay on the GitHub release; _redirects sends the downloads there."""

from __future__ import annotations

import argparse
import gzip
import hashlib
import io
import json
import re
import shutil
import subprocess
import tarfile
from email.utils import formatdate
from pathlib import Path

REPOSITORY = "johnkozaris/Kodosi"


def file_sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def bytes_sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def fail(message: str) -> None:
    raise SystemExit(f"generate-update-site: {message}")


def deb_control(path: Path) -> str:
    data = path.read_bytes()
    if not data.startswith(b"!<arch>\n"):
        fail(f"{path.name} is not a Debian package")
    offset = 8
    while offset + 60 <= len(data):
        header = data[offset : offset + 60]
        name = header[:16].decode("ascii").strip().rstrip("/")
        size = int(header[48:58].decode("ascii").strip())
        body = data[offset + 60 : offset + 60 + size]
        offset += 60 + size + size % 2
        if not name.startswith("control.tar"):
            continue
        if name.endswith(".zst"):
            from compression import zstd

            body = zstd.decompress(body)
        with tarfile.open(fileobj=io.BytesIO(body), mode="r:*") as archive:
            for member in archive.getmembers():
                if member.name in ("control", "./control"):
                    control = archive.extractfile(member)
                    if control:
                        return control.read().decode("utf-8").strip()
    fail(f"{path.name} has no control file")
    return ""


def apt_repository(deb: Path, version: str, apt: Path) -> None:
    control = deb_control(deb)
    fields = dict(re.findall(r"^([A-Za-z-]+): (.*)$", control, re.MULTILINE))
    if fields.get("Version") != version or fields.get("Architecture") != "amd64":
        fail(f"{deb.name} is not version {version} for amd64")
    stanza = (
        f"{control}\nFilename: pool/{version}/{deb.name}\n"
        f"Size: {deb.stat().st_size}\nSHA256: {file_sha256(deb)}\n"
    ).encode("utf-8")
    index = apt / "dists/stable/main/binary-amd64"
    index.mkdir(parents=True)
    compressed = gzip.compress(stanza, mtime=0)
    (index / "Packages").write_bytes(stanza)
    (index / "Packages.gz").write_bytes(compressed)
    listing = "".join(
        f" {bytes_sha256(data)} {len(data)} main/binary-amd64/{name}\n"
        for name, data in (("Packages", stanza), ("Packages.gz", compressed))
    )
    (apt / "dists/stable/Release").write_text(
        "Origin: Kodosi\nLabel: Kodosi\nSuite: stable\nCodename: stable\n"
        f"Date: {formatdate(usegmt=True)}\nArchitectures: amd64\nComponents: main\n"
        f"Description: Kodosi\nSHA256:\n{listing}",
        encoding="utf-8",
    )


def sign_release(apt: Path, fingerprint: str) -> None:
    release = apt / "dists/stable/Release"
    subprocess.run(
        ["gpg", "--batch", "--no-tty", "--yes", "--pinentry-mode", "error", "--digest-algo", "SHA512",
         "--local-user", f"{fingerprint}!", "--clearsign", "--output", str(release.with_name("InRelease")),
         str(release)],
        check=True,
    )


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("release", type=Path, help="the folder with the files of the release")
    parser.add_argument("site", type=Path, help="the folder of the update site to write")
    parser.add_argument("--version", required=True)
    parser.add_argument("--appcast", type=Path, required=True, help="appcast.xml from generate-appcast.sh")
    parser.add_argument("--apt-key", type=Path, required=True, help="the public apt archive key")
    parser.add_argument("--sign", metavar="FINGERPRINT", help="sign the apt index with this OpenPGP key")
    arguments = parser.parse_args()
    version, release, site = arguments.version, arguments.release, arguments.site
    if not re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+", version):
        fail("the version must be MAJOR.MINOR.PATCH")
    files = {
        "macos": release / f"Kodosi-{version}.dmg",
        "linux": release / f"kodosi_{version}_amd64.deb",
        "linuxArchive": release / f"kodosi-{version}-linux-x86_64.tar.gz",
        "archLinux": release / f"kodosi-bin-{version}-1-x86_64.pkg.tar.zst",
    }
    for path in files.values():
        if not path.is_file():
            fail(f"{path} is missing")
    if site.exists():
        fail(f"{site} exists; give a new folder")
    sums = release / "SHA256SUMS"
    sums.unlink(missing_ok=True)
    sums.write_text(
        "".join(f"{file_sha256(path)}  {path.name}\n" for path in sorted(release.iterdir()) if path.is_file()),
        encoding="utf-8",
    )
    download = f"https://github.com/{REPOSITORY}/releases/download/v{version}"
    (site / "macos").mkdir(parents=True)
    shutil.copyfile(arguments.appcast, site / "macos/appcast.xml")
    apt = site / "apt"
    apt_repository(files["linux"], version, apt)
    shutil.copyfile(arguments.apt_key, apt / "kodosi-archive-keyring.pgp")
    if arguments.sign:
        sign_release(apt, arguments.sign)
    (site / "release.json").write_text(
        json.dumps(
            {
                "version": version,
                "files": {
                    platform: {"name": path.name, "url": f"{download}/{path.name}", "sha256": file_sha256(path)}
                    for platform, path in files.items()
                },
            },
            indent=2,
        )
        + "\n",
        encoding="utf-8",
    )
    (site / "_redirects").write_text(
        f"/apt/pool/:version/:file https://github.com/{REPOSITORY}/releases/download/v:version/:file 302\n"
        f"/download/macos {download}/{files['macos'].name} 302\n"
        f"/download/linux {download}/{files['linux'].name} 302\n",
        encoding="utf-8",
    )
    print(f"[*] Wrote the update site of {version} in {site}")


if __name__ == "__main__":
    main()
