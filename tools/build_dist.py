#!/usr/bin/env python3
"""Package an existing native Cargo build and preserve dependency license notices."""
import argparse
import json
from pathlib import Path
import plistlib
import shutil
import subprocess
import zipfile

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--target", required=True)
    args = parser.parse_args()
    target = args.target
    executable = "opp-viewer.exe" if "windows" in target else "opp-viewer"
    source = ROOT / "target" / target / "release" / executable
    if not source.is_file():
        raise SystemExit(f"Build the release target first: {target}")
    destination = ROOT / "dist" / f"opp-viewer-0.1.0-{target}"
    destination.mkdir(parents=True, exist_ok=True)
    if "apple" in target:
        app = destination / "OPP Viewer.app" / "Contents"
        (app / "MacOS").mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, app / "MacOS" / "opp-viewer")
        with (app / "Info.plist").open("wb") as handle:
            plistlib.dump({"CFBundleExecutable": "opp-viewer", "CFBundleIdentifier": "org.openpartprotocol.viewer", "CFBundleName": "OPP Viewer", "CFBundleDisplayName": "OPP Viewer", "CFBundlePackageType": "APPL", "CFBundleVersion": "0.1.0", "CFBundleShortVersionString": "0.1.0", "NSHighResolutionCapable": True, "LSMinimumSystemVersion": "12.0"}, handle)
    else:
        shutil.copy2(source, destination / executable)
    for name in ["LICENSE", "README.md", "THIRD_PARTY_NOTICES.md"]:
        shutil.copy2(ROOT / name, destination / name)
    shutil.copytree(ROOT / "fixtures", destination / "examples", dirs_exist_ok=True)
    metadata = json.loads(subprocess.check_output(["cargo", "metadata", "--locked", "--offline", "--format-version", "1", "--filter-platform", target], cwd=ROOT, text=True))
    notices = destination / "dependency-licenses"
    notices.mkdir(exist_ok=True)
    declarations = []
    for package in metadata["packages"]:
        if package["name"] == "opp-viewer":
            continue
        source_directory = Path(package["manifest_path"]).parent
        key = f"{package['name']}-{package['version']}"
        declarations.append({"package": key, "license": package.get("license"), "repository": package.get("repository")})
        matches = [p for p in source_directory.rglob("*") if p.is_file() and p.name.lower().startswith(("license", "licence", "copying", "notice")) and not any(part in {"target", ".git"} for part in p.parts)]
        for path in matches:
            output = notices / key / path.relative_to(source_directory)
            output.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(path, output)
        if package.get("license_file"):
            path = source_directory / package["license_file"]
            if path.is_file():
                output = notices / key / path.name
                output.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(path, output)
    (notices / "inventory.json").write_text(json.dumps(declarations, indent=2) + "\n", encoding="utf-8")
    output = destination.parent / f"{destination.name}.zip"
    with zipfile.ZipFile(output, "w", zipfile.ZIP_DEFLATED) as archive:
        for path in sorted(destination.rglob("*")):
            if path.is_file():
                archive.write(path, path.relative_to(destination.parent))
    print(output)


if __name__ == "__main__":
    main()
