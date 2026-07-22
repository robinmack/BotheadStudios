// A minimal PNG reader for rigs, so a visual claim can be checked against PIXELS.
//
// Why this exists: a WebGPU canvas cannot be read back with `drawImage` into a 2D context (the swapchain
// texture is gone after present — the sample comes out solid black), so the only honest pixel source is
// Playwright's compositor screenshot, which arrives as a PNG. Comparing those PNGs by BYTE LENGTH is what
// this rig did before, and it lies: PNG size tracks image complexity, so a rotated globe can compress to
// almost the same size and read as "nothing moved".
//
// Scope: 8-bit non-interlaced RGB/RGBA/grey — which is what Playwright emits. Anything else throws rather
// than guessing.
import zlib from 'node:zlib';

const paeth = (a, b, c) => {
  const p = a + b - c, pa = Math.abs(p - a), pb = Math.abs(p - b), pc = Math.abs(p - c);
  return pa <= pb && pa <= pc ? a : pb <= pc ? b : c;
};

/** Decode a PNG buffer -> { width, height, channels, data } with one byte per channel per pixel. */
export function decodePng(buf) {
  if (buf.readUInt32BE(0) !== 0x89504e47) throw new Error('not a PNG');
  let off = 8, width = 0, height = 0, bitDepth = 0, colorType = 0;
  const idat = [];
  while (off < buf.length) {
    const len = buf.readUInt32BE(off);
    const type = buf.toString('ascii', off + 4, off + 8);
    const body = buf.subarray(off + 8, off + 8 + len);
    if (type === 'IHDR') {
      width = body.readUInt32BE(0);
      height = body.readUInt32BE(4);
      bitDepth = body[8];
      colorType = body[9];
      if (body[12] !== 0) throw new Error('interlaced PNG unsupported');
    } else if (type === 'IDAT') idat.push(body);
    else if (type === 'IEND') break;
    off += 12 + len;
  }
  if (bitDepth !== 8) throw new Error(`bit depth ${bitDepth} unsupported`);
  const channels = { 0: 1, 2: 3, 4: 2, 6: 4 }[colorType];
  if (!channels) throw new Error(`color type ${colorType} unsupported`);

  const raw = zlib.inflateSync(Buffer.concat(idat));
  const stride = width * channels;
  const out = Buffer.alloc(height * stride);
  for (let y = 0; y < height; y++) {
    const filter = raw[y * (stride + 1)];
    const src = raw.subarray(y * (stride + 1) + 1, y * (stride + 1) + 1 + stride);
    const cur = out.subarray(y * stride, (y + 1) * stride);
    const prev = y > 0 ? out.subarray((y - 1) * stride, y * stride) : null;
    for (let i = 0; i < stride; i++) {
      const a = i >= channels ? cur[i - channels] : 0;
      const b = prev ? prev[i] : 0;
      const c = prev && i >= channels ? prev[i - channels] : 0;
      const x = src[i];
      cur[i] =
        filter === 0 ? x
        : filter === 1 ? (x + a) & 255
        : filter === 2 ? (x + b) & 255
        : filter === 3 ? (x + ((a + b) >> 1)) & 255
        : filter === 4 ? (x + paeth(a, b, c)) & 255
        : (() => { throw new Error(`bad filter ${filter}`); })();
    }
  }
  return { width, height, channels, data: out };
}

/** Mean luminance 0..255 — a blank render reads ~0, which is never evidence of anything. */
export function meanLevel(img) {
  let s = 0;
  for (let i = 0; i < img.data.length; i++) s += img.data[i];
  return s / img.data.length;
}

/** Mean absolute per-channel difference between two same-size images, 0..255. */
export function meanDiff(a, x) {
  if (a.data.length !== x.data.length) throw new Error('size mismatch');
  let s = 0;
  for (let i = 0; i < a.data.length; i++) s += Math.abs(a.data[i] - x.data[i]);
  return s / a.data.length;
}
