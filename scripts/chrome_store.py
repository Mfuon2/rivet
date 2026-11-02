#!/usr/bin/env python3
"""Verify a stable GitHub release and submit its ZIP to the existing Chrome item."""
import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
import zipfile

import release
from publish import gh, version_tuple

ITEM = "oloipikeghldaokdiglloelbpfheeeik"
API = "https://chromewebstore.googleapis.com"
FIELDS = ("CWS_PUBLISHER_ID", "CWS_CLIENT_ID", "CWS_CLIENT_SECRET", "CWS_REFRESH_TOKEN")


def report(message):
    print(message)
    if os.environ.get("GITHUB_STEP_SUMMARY"):
        with open(os.environ["GITHUB_STEP_SUMMARY"], "a", encoding="utf-8") as stream:
            stream.write("\n### Chrome Web Store\n\n" + message + "\n")


def configured():
    values = {name: os.environ.get(name, "").strip() for name in FIELDS}
    present = [name for name, value in values.items() if value]
    if not present:
        report("Publisher setup required: GitHub packages are published; Chrome Store submission was skipped. Configure CWS_PUBLISHER_ID and the three CWS OAuth secrets described in release/README.md.")
        ready = False
    elif len(present) != len(FIELDS):
        raise ValueError("Incomplete publisher setup; missing: " + ", ".join(name for name in FIELDS if not values[name]))
    else:
        if not re.fullmatch(r"[A-Za-z0-9_-]{1,200}", values["CWS_PUBLISHER_ID"]):
            raise ValueError("Invalid Chrome publisher ID")
        ready = True
    if os.environ.get("GITHUB_OUTPUT"):
        with open(os.environ["GITHUB_OUTPUT"], "a", encoding="utf-8") as stream:
            stream.write(f"configured={str(ready).lower()}\n")
    return ready


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        return None


def request(url, data=None, token=None, content_type="application/json"):
    # Only fixed Google HTTPS endpoints may receive publisher credentials.
    parsed = urllib.parse.urlsplit(url)
    if parsed.scheme != "https" or parsed.netloc not in {"oauth2.googleapis.com", "chromewebstore.googleapis.com"}:
        raise ValueError("Untrusted store API endpoint")
    headers = {"Accept": "application/json", "Content-Type": content_type}
    if token:
        headers["Authorization"] = "Bearer " + token
    req = urllib.request.Request(url, data=data, headers=headers)
    try:
        with urllib.request.build_opener(NoRedirect).open(req, timeout=60) as response:
            body = response.read(2 * 1024 * 1024 + 1)
        if len(body) > 2 * 1024 * 1024:
            raise ValueError("Store response exceeds limit")
        return json.loads(body)
    except urllib.error.HTTPError as error:
        # Do not echo request bodies, tokens or server error text into logs.
        raise ValueError(f"Google API returned HTTP {error.code}; inspect the publisher dashboard or OAuth configuration before retrying") from None
    except (urllib.error.URLError, TimeoutError, json.JSONDecodeError):
        raise ValueError("Store request failed; check the dashboard before retrying an upload or submission") from None


def download(repo, tag, directory):
    if not re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", repo):
        raise ValueError("Invalid repository")
    if not tag:
        tag = json.loads(gh("api", f"repos/{repo}/releases/latest"))["tag_name"]
    if tag != "v" + ".".join(map(str, version_tuple(tag))):
        raise ValueError("Expected a numeric release tag")
    metadata = json.loads(gh("api", f"repos/{repo}/releases/tags/{tag}"))
    if metadata["draft"] or metadata["prerelease"]:
        raise ValueError("Only published stable releases can be submitted automatically")
    directory.mkdir(parents=True, exist_ok=True)
    if any(directory.iterdir()):
        raise ValueError("Use an empty directory for release verification")
    gh("release", "download", tag, "--repo", repo, "--dir", directory)
    release.verify(directory, release.PUBLIC_KEY)
    value = tag[1:]
    package = directory / f"rivet-{value}-chrome-store.zip"
    validate_package(package, value)
    (directory / "store-version.txt").write_text(value, encoding="ascii")
    print(f"Authenticated Chrome store package for {tag}")


def validate_package(package, value):
    version_tuple(value)
    if package.is_symlink() or package.stat().st_size > 25 * 1024 * 1024:
        raise ValueError("Invalid store package")
    with zipfile.ZipFile(package) as archive:
        infos = [entry for entry in archive.infolist() if entry.filename == "manifest.json"]
        if len(infos) != 1 or infos[0].file_size > 65536:
            raise ValueError("Invalid store manifest")
        manifest = json.loads(archive.read(infos[0]))
    if manifest.get("version") != value or "key" in manifest or manifest.get("manifest_version") != 3:
        raise ValueError("Store manifest version, format or identity does not match the release")


def versions(revision):
    return {channel.get("crxVersion") for channel in revision.get("distributionChannels", [])}


def submit(package, value, api=request, pause=time.sleep):
    validate_package(package, value)
    publisher = os.environ["CWS_PUBLISHER_ID"]
    if not re.fullmatch(r"[A-Za-z0-9_-]{1,200}", publisher):
        raise ValueError("Invalid Chrome publisher ID")
    payload = urllib.parse.urlencode({"client_id": os.environ["CWS_CLIENT_ID"], "client_secret": os.environ["CWS_CLIENT_SECRET"], "refresh_token": os.environ["CWS_REFRESH_TOKEN"], "grant_type": "refresh_token"}).encode()
    token = api("https://oauth2.googleapis.com/token", data=payload, content_type="application/x-www-form-urlencoded").get("access_token")
    if not isinstance(token, str) or not token:
        raise ValueError("Google did not return an access token")
    name = f"publishers/{publisher}/items/{ITEM}"
    endpoint = API + "/v2/" + name
    status = api(endpoint + ":fetchStatus", token=token)
    if status.get("takenDown") or status.get("warned"):
        raise ValueError("Publisher policy issue requires attention in the Chrome dashboard")
    current = status.get("publishedItemRevisionStatus", {})
    pending = status.get("submittedItemRevisionStatus", {})
    published = versions(current)
    if current.get("state") == "PUBLISHED" and value in published:
        report(f"Chrome extension {value} is already published; no upload repeated.")
        return
    for existing in published:
        if version_tuple(existing) > version_tuple(value):
            raise ValueError("Refusing to submit a version older than the published Chrome extension")
    if pending.get("state") in {"PENDING_REVIEW", "STAGED"}:
        if value in versions(pending):
            report(f"Chrome extension {value}: {pending['state']}. No duplicate upload or submission.")
            return
        raise ValueError("Another extension version is awaiting review/deployment; it was preserved. Retry this store job after that submission finishes")
    if pending.get("state") == "REJECTED" and value in versions(pending):
        raise ValueError("This version was rejected; address the review feedback before resubmission")
    uploaded = api(API + "/upload/v2/" + name + ":upload?uploadType=media", data=package.read_bytes(), token=token, content_type="application/zip")
    state = uploaded.get("uploadState")
    if uploaded.get("crxVersion") and uploaded["crxVersion"] != value:
        raise ValueError("Google upload version does not match the verified ZIP")
    for _ in range(24):
        if state not in {"IN_PROGRESS", "UPLOAD_IN_PROGRESS"}:
            break
        pause(5)
        state = api(endpoint + ":fetchStatus", token=token).get("lastAsyncUploadState")
    if state != "SUCCEEDED":
        raise ValueError("Chrome upload did not succeed; inspect the dashboard before retrying")
    result = api(endpoint + ":publish", data=json.dumps({"publishType": "DEFAULT_PUBLISH", "skipReview": False, "blockOnWarnings": True}).encode(), token=token)
    state = result.get("state")
    if state not in {"PENDING_REVIEW", "PUBLISHED", "PUBLISHED_TO_TESTERS", "STAGED"}:
        raise ValueError("Submission was not confirmed; inspect the dashboard before retrying")
    report(f"Chrome extension {value}: {state}. Google controls review/approval. DEFAULT_PUBLISH requests public deployment after approval; existing users receive the update through Chrome.")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    sub.add_parser("config")
    p = sub.add_parser("download"); p.add_argument("directory", type=Path); p.add_argument("--repo", required=True); p.add_argument("--tag", default="")
    p = sub.add_parser("submit"); p.add_argument("directory", type=Path)
    args = parser.parse_args()
    try:
        if args.command == "config": configured()
        elif args.command == "download": download(args.repo, args.tag, args.directory)
        elif args.command == "submit":
            if not configured():
                raise ValueError("Publisher credentials are required to submit")
            value = (args.directory / "store-version.txt").read_text(encoding="ascii").strip()
            # Reverify immediately before the external mutation, including signed checksums.
            release.verify(args.directory, release.PUBLIC_KEY)
            submit(args.directory / f"rivet-{value}-chrome-store.zip", value)
    except (ValueError, OSError, KeyError, zipfile.BadZipFile, subprocess.CalledProcessError) as error:
        parser.exit(1, f"Chrome Store publication stopped: {error}\n")


if __name__ == "__main__":
    main()
