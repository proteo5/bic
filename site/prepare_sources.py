"""Download and prepare the example images for the demo page.

Every example comes from Wikimedia Commons under a license that allows
republishing. The licence and author of each file are recorded in
sources/manifest.json and shown on the page. Images larger than MAX_SIDE are
resized (the page says so), so the demo stays small.

Usage: python site/prepare_sources.py
"""

import json
import os
import time
import urllib.parse
import urllib.request

from PIL import Image

HERE = os.path.dirname(os.path.abspath(__file__))
SOURCES = os.path.join(HERE, "sources")
RAW = os.path.join(HERE, "raw")
MAX_SIDE = 1600
USER_AGENT = "bic-demo (https://github.com/proteo5/bic)"

# id, Commons file name, kind ("flat" or "photo"), short description.
EXAMPLES = [
    ("ui-screenshot", "Itemis_ANALYZE_user-interface.png", "flat",
     "Software user interface (screenshot)"),
    ("flowchart", "Flowchart_of_a_code_health_checker_program.png", "flat",
     "Flowchart diagram"),
    ("bar-chart", "Bar_Chart_Showing_South_African_Internet_Usage_by_Year.png", "flat",
     "Bar chart"),
    ("icon", "Search-icon-transparent-background.png", "flat",
     "Icon with transparency"),
    ("pixel-art", "Atari_STF_demo_scene_1989-1994_Jovis_-_Pixel_art.png", "flat",
     "Pixel art"),
    ("photo-landscape-1", "Landscape_Arnisee-region.JPG", "photo",
     "Landscape photograph"),
    ("photo-landscape-2", "Tuscan_landscape_with_lonely_tree.jpg", "photo",
     "Landscape photograph"),
]


def commons_metadata(file_names):
    titles = "|".join("File:" + urllib.parse.unquote(n) for n in file_names)
    url = (
        "https://commons.wikimedia.org/w/api.php?action=query&titles="
        + urllib.parse.quote(titles, safe="|:")
        + "&prop=imageinfo&iiprop=url|extmetadata&format=json&formatversion=2"
    )
    data = fetch(url, binary=False)
    pages = json.loads(data)["query"]["pages"]
    result = {}
    for page in pages:
        if "imageinfo" not in page:
            continue
        info = page["imageinfo"][0]
        meta = info.get("extmetadata", {})
        name = page["title"][len("File:"):].replace(" ", "_")
        result[name] = {
            "download_url": info["url"].split("?")[0],
            "page_url": info["descriptionurl"],
            "license": meta.get("LicenseShortName", {}).get("value", "unknown"),
            "license_url": meta.get("LicenseUrl", {}).get("value", ""),
            "author": strip_html(meta.get("Artist", {}).get("value", "")),
        }
    return result


def strip_html(text):
    import re

    return re.sub(r"\s+", " ", re.sub(r"<[^>]+>", "", text)).strip()


def fetch(url, binary=True):
    for attempt in range(12):
        try:
            request = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
            data = urllib.request.urlopen(request).read()
            return data if binary else data.decode("utf-8")
        except urllib.error.HTTPError as error:
            if error.code != 429:
                raise
            time.sleep(20 + 10 * attempt)
    raise RuntimeError("Wikimedia kept rate-limiting: " + url)


def prepare(example_id, file_name, kind, meta):
    raw_path = os.path.join(RAW, file_name)
    if not os.path.exists(raw_path):
        os.makedirs(RAW, exist_ok=True)
        with open(raw_path, "wb") as f:
            f.write(fetch(meta["download_url"]))
        time.sleep(5)
    image = Image.open(raw_path)
    original_size = image.size
    if max(image.size) > MAX_SIDE:
        scale = MAX_SIDE / max(image.size)
        size = (round(image.width * scale), round(image.height * scale))
        resample = Image.Resampling.LANCZOS if kind == "photo" else Image.Resampling.BOX
        image = image.convert("RGBA" if has_alpha(image) else "RGB").resize(size, resample)
    if kind == "photo":
        out = os.path.join(SOURCES, example_id + ".jpg")
        image.convert("RGB").save(out, "JPEG", quality=90, optimize=True)
    else:
        out = os.path.join(SOURCES, example_id + ".png")
        image.convert("RGBA" if has_alpha(image) else "RGB").save(out, "PNG", optimize=True)
    final = Image.open(out)
    return {
        "id": example_id,
        "file": os.path.basename(out),
        "kind": kind,
        "description": None,
        "width": final.width,
        "height": final.height,
        "original_width": original_size[0],
        "original_height": original_size[1],
        "license": meta["license"],
        "license_url": meta["license_url"],
        "author": meta["author"],
        "page_url": meta["page_url"],
        "source_file": file_name,
        "resized": max(original_size) > MAX_SIDE,
    }


def has_alpha(image):
    return image.mode in ("RGBA", "LA") or (image.mode == "P" and "transparency" in image.info)


def main():
    os.makedirs(SOURCES, exist_ok=True)
    metadata = commons_metadata([name for _, name, _, _ in EXAMPLES])
    manifest = []
    for example_id, file_name, kind, description in EXAMPLES:
        meta = metadata.get(file_name)
        if meta is None:
            raise SystemExit("not found on Commons: " + file_name)
        entry = prepare(example_id, file_name, kind, meta)
        entry["description"] = description
        manifest.append(entry)
        print(f"{example_id:20} {entry['width']}x{entry['height']} {entry['license']}")
    with open(os.path.join(SOURCES, "manifest.json"), "w", encoding="utf-8") as f:
        json.dump(manifest, f, ensure_ascii=False, indent=1)


if __name__ == "__main__":
    main()
