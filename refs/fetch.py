#!/usr/bin/env python3
"""Fetch available primary papers without bypassing publisher access controls."""
import concurrent.futures
import hashlib
import json
from pathlib import Path
import urllib.request

ROOT = Path(__file__).resolve().parent

def fetch(source):
    out = {"id": source["id"], "url": source["pdf"]}
    try:
        request = urllib.request.Request(source["pdf"], headers={"User-Agent": "packit-reference-fetch/1.0"})
        with urllib.request.urlopen(request, timeout=25) as response:
            data = response.read(20_000_001)
        if len(data) > 20_000_000 or not data.startswith(b"%PDF-"):
            raise ValueError("Not a PDF or larger than 20 MB")
        path = ROOT / "downloads" / (source["id"] + ".pdf")
        path.write_bytes(data)
        out.update(file=path.name, sha256=hashlib.sha256(data).hexdigest(), status="downloaded")
    except Exception as error:
        out.update(status="unavailable", error=str(error))
    return out

if __name__ == "__main__":
    (ROOT / "downloads").mkdir(exist_ok=True)
    sources = json.loads((ROOT / "sources.json").read_text())
    with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
        results = list(pool.map(fetch, [s for s in sources if s.get("pdf")]))
    (ROOT / "downloads" / "manifest.json").write_text(json.dumps(results, indent=2) + "\n")
    for result in results:
        print(result["id"], result["status"])
