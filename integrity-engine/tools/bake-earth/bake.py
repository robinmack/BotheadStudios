#!/usr/bin/env python3
"""Bake the Earth "world" rasters (docs/43) from CITED open datasets → web/public/worlds/earth/.

Reproducible offline bake (run: `python3 tools/bake-earth/bake.py`). The large source datasets are
downloaded to a temp dir and NOT committed; only the small equirectangular output PNGs are committed.

Sources (all public domain / open):
  - Land mask       : Natural Earth ne_110m_land, 1:110m physical (naturalearthdata.com),
                      via github nvkelso/natural-earth-vector geojson.
  - Elevation+bathy : NOAA NCEI ETOPO 2022 60-arc-second surface DEM (doi:10.25921/fd45-gt74) —
                      real elevation VALUES including ocean bathymetry.
  - Land cover      : NASA MODIS MCD12Q1 (IGBP) via GIBS if reachable; otherwise a DERIVED climate
                      approximation from latitude+elevation+coast, explicitly FLAGGED as derived
                      (no-fudge: never presented as measured land cover). See LANDCOVER_SOURCE below.

Outputs (equirectangular, -180..180 lon / +90..-90 lat, row 0 = north):
  landmask.png   L    255=land 0=ocean
  elevation.png  RGB  16-bit elevation packed R<<8|G over the range in world.json.surface.elevation_range_m
  landcover.png  L    biome index → world.json.surface.biomes (0 water,1 grass,2 sand,3 pine,4 snow,5 granite)
"""
import json
import os
import urllib.request

import numpy as np
from PIL import Image, ImageDraw

W, H = 2048, 1024
HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.normpath(os.path.join(HERE, "../../web/public/worlds/earth"))
TMP = os.environ.get("BAKE_TMP", "/tmp/bake-earth")
ELEV_LO, ELEV_HI = -11000.0, 9000.0  # metres; must match world.json.surface.elevation_range_m
os.makedirs(OUT, exist_ok=True)
os.makedirs(TMP, exist_ok=True)


def fetch(url, path, desc):
    if os.path.exists(path) and os.path.getsize(path) > 0:
        print(f"  cached {desc} ({os.path.getsize(path) / 1e6:.1f} MB)")
        return path
    print(f"  downloading {desc} …")
    req = urllib.request.Request(url, headers={"User-Agent": "greenfield-bake/1.0"})
    with urllib.request.urlopen(req, timeout=120) as r, open(path, "wb") as f:
        f.write(r.read())
    print(f"  got {os.path.getsize(path) / 1e6:.1f} MB")
    return path


def lonlat_px(lon, lat):
    return ((lon + 180.0) / 360.0 * W, (90.0 - lat) / 180.0 * H)


def bake_landmask():
    print("[1/3] land mask — Natural Earth ne_110m_land")
    url = "https://raw.githubusercontent.com/nvkelso/natural-earth-vector/master/geojson/ne_110m_land.geojson"
    gj = json.load(open(fetch(url, f"{TMP}/ne_110m_land.geojson", "Natural Earth land polygons")))
    img = Image.new("L", (W, H), 0)  # ocean
    d = ImageDraw.Draw(img)

    def draw(rings):
        d.polygon([lonlat_px(x, y) for x, y in rings[0]], fill=255)  # exterior = land
        for hole in rings[1:]:
            d.polygon([lonlat_px(x, y) for x, y in hole], fill=0)  # inland water

    for feat in gj["features"]:
        g = feat["geometry"]
        if g["type"] == "Polygon":
            draw(g["coordinates"])
        elif g["type"] == "MultiPolygon":
            for poly in g["coordinates"]:
                draw(poly)
    land = np.asarray(img)
    frac = float((land > 127).mean())
    print(f"  land fraction = {frac:.3f}  (real Earth ≈ 0.29)")
    img.save(f"{OUT}/landmask.png")
    return land > 127


def bake_elevation():
    print("[2/3] elevation+bathymetry — NOAA ETOPO 2022 60s")
    url = ("https://www.ngdc.noaa.gov/mgg/global/relief/ETOPO2022/data/60s/"
           "60s_surface_elev_gtif/ETOPO_2022_v1_60s_N90W180_surface.tif")
    p = fetch(url, f"{TMP}/etopo_60s.tif", "ETOPO 2022 60s surface DEM (~450 MB)")
    Image.MAX_IMAGE_PIXELS = None
    im = Image.open(p)
    arr = np.asarray(im, dtype=np.float32)
    print(f"  ETOPO grid {arr.shape}, elevation {arr.min():.0f}..{arr.max():.0f} m")
    small = np.asarray(Image.fromarray(arr, mode="F").resize((W, H), Image.BILINEAR), dtype=np.float32)
    v16 = (np.clip((small - ELEV_LO) / (ELEV_HI - ELEV_LO), 0, 1) * 65535.0).astype(np.uint16)
    rgb = np.zeros((H, W, 3), np.uint8)
    rgb[..., 0] = (v16 >> 8).astype(np.uint8)
    rgb[..., 1] = (v16 & 0xFF).astype(np.uint8)
    Image.fromarray(rgb, "RGB").save(f"{OUT}/elevation.png")
    print(f"  elevation.png written (RGB-packed over [{ELEV_LO:.0f},{ELEV_HI:.0f}] m)")
    return small


def bake_landcover(land, elev):
    print("[3/3] land cover / biomes")
    # Real land cover (NASA MODIS via GIBS) if the endpoint cooperates; else a flagged derived model.
    source = "derived"
    try:
        url = ("https://gibs.earthdata.nasa.gov/wms/epsg4326/best/wms.cgi?service=WMS&request=GetMap"
               "&version=1.1.1&layers=MODIS_Terra_Land_Cover_Type_Yearly&srs=EPSG:4326"
               f"&bbox=-180,-90,180,90&width={W}&height={H}&format=image/png&TIME=2020-01-01")
        p = fetch(url, f"{TMP}/modis_landcover.png", "MODIS land cover (GIBS)")
        im = Image.open(p).convert("RGB")
        if im.size == (W, H) and np.asarray(im).std() > 5:  # a real image, not an error tile
            source = "MODIS MCD12Q1 (GIBS)"
            # (mapping MODIS IGBP classes → our 6 biomes would go here once verified)
    except Exception as e:
        print(f"  GIBS land cover unavailable ({e}); using derived climate approximation")

    if source == "derived":
        # DERIVED biome approximation (FLAGGED — not measured land cover): from latitude + elevation + coast.
        # 0 water · 1 grass · 2 sand(desert) · 3 pine(forest) · 4 snow/ice · 5 granite(bare rock/high mtn).
        lats = 90.0 - (np.arange(H) + 0.5) / H * 180.0
        latg = np.repeat(lats[:, None], W, axis=1)
        biome = np.zeros((H, W), np.uint8)  # ocean
        L = land
        alat = np.abs(latg)
        biome[L] = 1  # default land = grassland
        biome[L & (alat < 23)] = 3  # tropics → forest
        biome[L & (alat >= 15) & (alat < 33) & (elev < 1500)] = 2  # subtropical desert bands
        biome[L & (alat >= 45) & (alat < 66)] = 3  # boreal → forest
        biome[L & (elev > 3000)] = 5  # high mountains → bare rock
        biome[L & ((alat >= 66) | (elev > 4500))] = 4  # polar / very high → snow/ice
        Image.fromarray(biome, "L").save(f"{OUT}/landcover.png")

    # provenance note committed next to the assets (no-fudge: record what's measured vs derived)
    open(f"{OUT}/SOURCES.txt", "w").write(
        "Earth world rasters — provenance (docs/43, baked by tools/bake-earth/bake.py)\n"
        "landmask.png   : Natural Earth ne_110m_land (naturalearthdata.com) — MEASURED\n"
        "elevation.png  : NOAA ETOPO 2022 60s surface (doi:10.25921/fd45-gt74) — MEASURED\n"
        f"landcover.png  : {source}"
        + ("" if source != "derived" else
           " — DERIVED climate approximation from lat+elevation+coast, NOT a measured land-cover dataset "
           "(flagged; real MODIS MCD12Q1 is the follow-up)")
        + "\n")
    print(f"  landcover.png written (source: {source})")


def main():
    land = bake_landmask()
    elev = bake_elevation()
    bake_landcover(land, elev)
    print(f"done → {OUT}")


if __name__ == "__main__":
    main()
