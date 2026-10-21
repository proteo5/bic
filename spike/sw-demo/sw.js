(() => {
  // node_modules/fzstd/esm/index.mjs
  var ab = ArrayBuffer;
  var u8 = Uint8Array;
  var u16 = Uint16Array;
  var i16 = Int16Array;
  var i32 = Int32Array;
  var slc = function(v, s, e) {
    if (u8.prototype.slice)
      return u8.prototype.slice.call(v, s, e);
    if (s == null || s < 0)
      s = 0;
    if (e == null || e > v.length)
      e = v.length;
    var n = new u8(e - s);
    n.set(v.subarray(s, e));
    return n;
  };
  var fill = function(v, n, s, e) {
    if (u8.prototype.fill)
      return u8.prototype.fill.call(v, n, s, e);
    if (s == null || s < 0)
      s = 0;
    if (e == null || e > v.length)
      e = v.length;
    for (; s < e; ++s)
      v[s] = n;
    return v;
  };
  var cpw = function(v, t, s, e) {
    if (u8.prototype.copyWithin)
      return u8.prototype.copyWithin.call(v, t, s, e);
    if (s == null || s < 0)
      s = 0;
    if (e == null || e > v.length)
      e = v.length;
    while (s < e) {
      v[t++] = v[s++];
    }
  };
  var ec = [
    "invalid zstd data",
    "window size too large (>2046MB)",
    "invalid block type",
    "FSE accuracy too high",
    "match distance too far back",
    "unexpected EOF"
  ];
  var err = function(ind, msg, nt) {
    var e = new Error(msg || ec[ind]);
    e.code = ind;
    if (Error.captureStackTrace)
      Error.captureStackTrace(e, err);
    if (!nt)
      throw e;
    return e;
  };
  var rb = function(d, b, n) {
    var i = 0, o = 0;
    for (; i < n; ++i)
      o |= d[b++] << (i << 3);
    return o;
  };
  var b4 = function(d, b) {
    return (d[b] | d[b + 1] << 8 | d[b + 2] << 16 | d[b + 3] << 24) >>> 0;
  };
  var rzfh = function(dat, w) {
    var n3 = dat[0] | dat[1] << 8 | dat[2] << 16;
    if (n3 == 3126568 && dat[3] == 253) {
      var flg = dat[4];
      var ss = flg >> 5 & 1, cc = flg >> 2 & 1, df = flg & 3, fcf = flg >> 6;
      if (flg & 8)
        err(0);
      var bt = 6 - ss;
      var db = df == 3 ? 4 : df;
      var di = rb(dat, bt, db);
      bt += db;
      var fsb = fcf ? 1 << fcf : ss;
      var fss = rb(dat, bt, fsb) + (fcf == 1 && 256);
      var ws = fss;
      if (!ss) {
        var wb = 1 << 10 + (dat[5] >> 3);
        ws = wb + (wb >> 3) * (dat[5] & 7);
      }
      if (ws > 2145386496)
        err(1);
      var buf = new u8((w == 1 ? fss || ws : w ? 0 : ws) + 12);
      buf[0] = 1, buf[4] = 4, buf[8] = 8;
      return {
        b: bt + fsb,
        y: 0,
        l: 0,
        d: di,
        w: w && w != 1 ? w : buf.subarray(12),
        e: ws,
        o: new i32(buf.buffer, 0, 3),
        u: fss,
        c: cc,
        m: Math.min(131072, ws)
      };
    } else if ((n3 >> 4 | dat[3] << 20) == 25481893) {
      return b4(dat, 4) + 8;
    }
    err(0);
  };
  var msb = function(val) {
    var bits = 0;
    for (; 1 << bits <= val; ++bits)
      ;
    return bits - 1;
  };
  var rfse = function(dat, bt, mal) {
    var tpos = (bt << 3) + 4;
    var al = (dat[bt] & 15) + 5;
    if (al > mal)
      err(3);
    var sz = 1 << al;
    var probs = sz, sym = -1, re = -1, i = -1, ht = sz;
    var buf = new ab(512 + (sz << 2));
    var freq = new i16(buf, 0, 256);
    var dstate = new u16(buf, 0, 256);
    var nstate = new u16(buf, 512, sz);
    var bb1 = 512 + (sz << 1);
    var syms = new u8(buf, bb1, sz);
    var nbits = new u8(buf, bb1 + sz);
    while (sym < 255 && probs > 0) {
      var bits = msb(probs + 1);
      var cbt = tpos >> 3;
      var msk = (1 << bits + 1) - 1;
      var val = (dat[cbt] | dat[cbt + 1] << 8 | dat[cbt + 2] << 16) >> (tpos & 7) & msk;
      var msk1fb = (1 << bits) - 1;
      var msv = msk - probs - 1;
      var sval = val & msk1fb;
      if (sval < msv)
        tpos += bits, val = sval;
      else {
        tpos += bits + 1;
        if (val > msk1fb)
          val -= msv;
      }
      freq[++sym] = --val;
      if (val == -1) {
        probs += val;
        syms[--ht] = sym;
      } else
        probs -= val;
      if (!val) {
        do {
          var rbt = tpos >> 3;
          re = (dat[rbt] | dat[rbt + 1] << 8) >> (tpos & 7) & 3;
          tpos += 2;
          sym += re;
        } while (re == 3);
      }
    }
    if (sym > 255 || probs)
      err(0);
    var sympos = 0;
    var sstep = (sz >> 1) + (sz >> 3) + 3;
    var smask = sz - 1;
    for (var s = 0; s <= sym; ++s) {
      var sf = freq[s];
      if (sf < 1) {
        dstate[s] = -sf;
        continue;
      }
      for (i = 0; i < sf; ++i) {
        syms[sympos] = s;
        do {
          sympos = sympos + sstep & smask;
        } while (sympos >= ht);
      }
    }
    if (sympos)
      err(0);
    for (i = 0; i < sz; ++i) {
      var ns = dstate[syms[i]]++;
      var nb = nbits[i] = al - msb(ns);
      nstate[i] = (ns << nb) - sz;
    }
    return [tpos + 7 >> 3, {
      b: al,
      s: syms,
      n: nbits,
      t: nstate
    }];
  };
  var rhu = function(dat, bt) {
    var i = 0, wc = -1;
    var buf = new u8(292), hb = dat[bt];
    var hw = buf.subarray(0, 256);
    var rc = buf.subarray(256, 268);
    var ri = new u16(buf.buffer, 268);
    if (hb < 128) {
      var _a = rfse(dat, bt + 1, 6), ebt = _a[0], fdt = _a[1];
      bt += hb;
      var epos = ebt << 3;
      var lb = dat[bt];
      if (!lb)
        err(0);
      var st1 = 0, st2 = 0, btr1 = fdt.b, btr2 = btr1;
      var fpos = (++bt << 3) - 8 + msb(lb);
      for (; ; ) {
        fpos -= btr1;
        if (fpos < epos)
          break;
        var cbt = fpos >> 3;
        st1 += (dat[cbt] | dat[cbt + 1] << 8) >> (fpos & 7) & (1 << btr1) - 1;
        hw[++wc] = fdt.s[st1];
        fpos -= btr2;
        if (fpos < epos)
          break;
        cbt = fpos >> 3;
        st2 += (dat[cbt] | dat[cbt + 1] << 8) >> (fpos & 7) & (1 << btr2) - 1;
        hw[++wc] = fdt.s[st2];
        btr1 = fdt.n[st1];
        st1 = fdt.t[st1];
        btr2 = fdt.n[st2];
        st2 = fdt.t[st2];
      }
      if (++wc > 255)
        err(0);
    } else {
      wc = hb - 127;
      for (; i < wc; i += 2) {
        var byte = dat[++bt];
        hw[i] = byte >> 4;
        hw[i + 1] = byte & 15;
      }
      ++bt;
    }
    var wes = 0;
    for (i = 0; i < wc; ++i) {
      var wt = hw[i];
      if (wt > 11)
        err(0);
      wes += wt && 1 << wt - 1;
    }
    var mb = msb(wes) + 1;
    var ts = 1 << mb;
    var rem = ts - wes;
    if (rem & rem - 1)
      err(0);
    hw[wc++] = msb(rem) + 1;
    for (i = 0; i < wc; ++i) {
      var wt = hw[i];
      ++rc[hw[i] = wt && mb + 1 - wt];
    }
    var hbuf = new u8(ts << 1);
    var syms = hbuf.subarray(0, ts), nb = hbuf.subarray(ts);
    ri[mb] = 0;
    for (i = mb; i > 0; --i) {
      var pv = ri[i];
      fill(nb, i, pv, ri[i - 1] = pv + rc[i] * (1 << mb - i));
    }
    if (ri[0] != ts)
      err(0);
    for (i = 0; i < wc; ++i) {
      var bits = hw[i];
      if (bits) {
        var code = ri[bits];
        fill(syms, i, code, ri[bits] = code + (1 << mb - bits));
      }
    }
    return [bt, {
      n: nb,
      b: mb,
      s: syms
    }];
  };
  var dllt = rfse(/* @__PURE__ */ new u8([
    81,
    16,
    99,
    140,
    49,
    198,
    24,
    99,
    12,
    33,
    196,
    24,
    99,
    102,
    102,
    134,
    70,
    146,
    4
  ]), 0, 6)[1];
  var dmlt = rfse(/* @__PURE__ */ new u8([
    33,
    20,
    196,
    24,
    99,
    140,
    33,
    132,
    16,
    66,
    8,
    33,
    132,
    16,
    66,
    8,
    33,
    68,
    68,
    68,
    68,
    68,
    68,
    68,
    68,
    36,
    9
  ]), 0, 6)[1];
  var doct = rfse(/* @__PURE__ */ new u8([
    32,
    132,
    16,
    66,
    102,
    70,
    68,
    68,
    68,
    68,
    36,
    73,
    2
  ]), 0, 5)[1];
  var b2bl = function(b, s) {
    var len = b.length, bl = new i32(len);
    for (var i = 0; i < len; ++i) {
      bl[i] = s;
      s += 1 << b[i];
    }
    return bl;
  };
  var llb = /* @__PURE__ */ new u8((/* @__PURE__ */ new i32([
    0,
    0,
    0,
    0,
    16843009,
    50528770,
    134678020,
    202050057,
    269422093
  ])).buffer, 0, 36);
  var llbl = /* @__PURE__ */ b2bl(llb, 0);
  var mlb = /* @__PURE__ */ new u8((/* @__PURE__ */ new i32([
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    16843009,
    50528770,
    117769220,
    185207048,
    252579084,
    16
  ])).buffer, 0, 53);
  var mlbl = /* @__PURE__ */ b2bl(mlb, 3);
  var dhu = function(dat, out, hu) {
    var len = dat.length, ss = out.length, lb = dat[len - 1], msk = (1 << hu.b) - 1, eb = -hu.b;
    if (!lb)
      err(0);
    var st = 0, btr = hu.b, pos = (len << 3) - 8 + msb(lb) - btr, i = -1;
    for (; pos > eb && i < ss; ) {
      var cbt = pos >> 3;
      var val = (dat[cbt] | dat[cbt + 1] << 8 | dat[cbt + 2] << 16) >> (pos & 7);
      st = (st << btr | val) & msk;
      out[++i] = hu.s[st];
      pos -= btr = hu.n[st];
    }
    if (pos != eb || i + 1 != ss)
      err(0);
  };
  var dhu4 = function(dat, out, hu) {
    var bt = 6;
    var ss = out.length, sz1 = ss + 3 >> 2, sz2 = sz1 << 1, sz3 = sz1 + sz2;
    dhu(dat.subarray(bt, bt += dat[0] | dat[1] << 8), out.subarray(0, sz1), hu);
    dhu(dat.subarray(bt, bt += dat[2] | dat[3] << 8), out.subarray(sz1, sz2), hu);
    dhu(dat.subarray(bt, bt += dat[4] | dat[5] << 8), out.subarray(sz2, sz3), hu);
    dhu(dat.subarray(bt), out.subarray(sz3), hu);
  };
  var rzb = function(dat, st, out) {
    var _a;
    var bt = st.b;
    var b0 = dat[bt], btype = b0 >> 1 & 3;
    st.l = b0 & 1;
    var sz = b0 >> 3 | dat[bt + 1] << 5 | dat[bt + 2] << 13;
    var ebt = (bt += 3) + sz;
    if (btype == 1) {
      if (bt >= dat.length)
        return;
      st.b = bt + 1;
      if (out) {
        fill(out, dat[bt], st.y, st.y += sz);
        return out;
      }
      return fill(new u8(sz), dat[bt]);
    }
    if (ebt > dat.length)
      return;
    if (btype == 0) {
      st.b = ebt;
      if (out) {
        out.set(dat.subarray(bt, ebt), st.y);
        st.y += sz;
        return out;
      }
      return slc(dat, bt, ebt);
    }
    if (btype == 2) {
      var b3 = dat[bt], lbt = b3 & 3, sf = b3 >> 2 & 3;
      var lss = b3 >> 4, lcs = 0, s4 = 0;
      if (lbt < 2) {
        if (sf & 1)
          lss |= dat[++bt] << 4 | (sf & 2 && dat[++bt] << 12);
        else
          lss = b3 >> 3;
      } else {
        s4 = sf;
        if (sf < 2)
          lss |= (dat[++bt] & 63) << 4, lcs = dat[bt] >> 6 | dat[++bt] << 2;
        else if (sf == 2)
          lss |= dat[++bt] << 4 | (dat[++bt] & 3) << 12, lcs = dat[bt] >> 2 | dat[++bt] << 6;
        else
          lss |= dat[++bt] << 4 | (dat[++bt] & 63) << 12, lcs = dat[bt] >> 6 | dat[++bt] << 2 | dat[++bt] << 10;
      }
      ++bt;
      var buf = out ? out.subarray(st.y, st.y + st.m) : new u8(st.m);
      var spl = buf.length - lss;
      if (lbt == 0)
        buf.set(dat.subarray(bt, bt += lss), spl);
      else if (lbt == 1)
        fill(buf, dat[bt++], spl);
      else {
        var hu = st.h;
        if (lbt == 2) {
          var hud = rhu(dat, bt);
          lcs += bt - (bt = hud[0]);
          st.h = hu = hud[1];
        } else if (!hu)
          err(0);
        (s4 ? dhu4 : dhu)(dat.subarray(bt, bt += lcs), buf.subarray(spl), hu);
      }
      var ns = dat[bt++];
      if (ns) {
        if (ns == 255)
          ns = (dat[bt++] | dat[bt++] << 8) + 32512;
        else if (ns > 127)
          ns = ns - 128 << 8 | dat[bt++];
        var scm = dat[bt++];
        if (scm & 3)
          err(0);
        var dts = [dmlt, doct, dllt];
        for (var i = 2; i > -1; --i) {
          var md = scm >> (i << 1) + 2 & 3;
          if (md == 1) {
            var rbuf = new u8([0, 0, dat[bt++]]);
            dts[i] = {
              s: rbuf.subarray(2, 3),
              n: rbuf.subarray(0, 1),
              t: new u16(rbuf.buffer, 0, 1),
              b: 0
            };
          } else if (md == 2) {
            _a = rfse(dat, bt, 9 - (i & 1)), bt = _a[0], dts[i] = _a[1];
          } else if (md == 3) {
            if (!st.t)
              err(0);
            dts[i] = st.t[i];
          }
        }
        var _b = st.t = dts, mlt = _b[0], oct = _b[1], llt = _b[2];
        var lb = dat[ebt - 1];
        if (!lb)
          err(0);
        var spos = (ebt << 3) - 8 + msb(lb) - llt.b, cbt = spos >> 3, oubt = 0;
        var lst = (dat[cbt] | dat[cbt + 1] << 8) >> (spos & 7) & (1 << llt.b) - 1;
        cbt = (spos -= oct.b) >> 3;
        var ost = (dat[cbt] | dat[cbt + 1] << 8) >> (spos & 7) & (1 << oct.b) - 1;
        cbt = (spos -= mlt.b) >> 3;
        var mst = (dat[cbt] | dat[cbt + 1] << 8) >> (spos & 7) & (1 << mlt.b) - 1;
        for (++ns; --ns; ) {
          var llc = llt.s[lst];
          var lbtr = llt.n[lst];
          var mlc = mlt.s[mst];
          var mbtr = mlt.n[mst];
          var ofc = oct.s[ost];
          var obtr = oct.n[ost];
          cbt = (spos -= ofc) >> 3;
          var ofp = 1 << ofc;
          var off = ofp + ((dat[cbt] | dat[cbt + 1] << 8 | dat[cbt + 2] << 16 | dat[cbt + 3] << 24) >>> (spos & 7) & ofp - 1);
          cbt = (spos -= mlb[mlc]) >> 3;
          var ml = mlbl[mlc] + ((dat[cbt] | dat[cbt + 1] << 8 | dat[cbt + 2] << 16) >> (spos & 7) & (1 << mlb[mlc]) - 1);
          cbt = (spos -= llb[llc]) >> 3;
          var ll = llbl[llc] + ((dat[cbt] | dat[cbt + 1] << 8 | dat[cbt + 2] << 16) >> (spos & 7) & (1 << llb[llc]) - 1);
          cbt = (spos -= lbtr) >> 3;
          lst = llt.t[lst] + ((dat[cbt] | dat[cbt + 1] << 8) >> (spos & 7) & (1 << lbtr) - 1);
          cbt = (spos -= mbtr) >> 3;
          mst = mlt.t[mst] + ((dat[cbt] | dat[cbt + 1] << 8) >> (spos & 7) & (1 << mbtr) - 1);
          cbt = (spos -= obtr) >> 3;
          ost = oct.t[ost] + ((dat[cbt] | dat[cbt + 1] << 8) >> (spos & 7) & (1 << obtr) - 1);
          if (off > 3) {
            st.o[2] = st.o[1];
            st.o[1] = st.o[0];
            st.o[0] = off -= 3;
          } else {
            var idx = off - (ll != 0);
            if (idx) {
              off = idx == 3 ? st.o[0] - 1 : st.o[idx];
              if (idx > 1)
                st.o[2] = st.o[1];
              st.o[1] = st.o[0];
              st.o[0] = off;
            } else
              off = st.o[0];
          }
          for (var i = 0; i < ll; ++i) {
            buf[oubt + i] = buf[spl + i];
          }
          oubt += ll, spl += ll;
          var stin = oubt - off;
          if (stin < 0) {
            var len = -stin;
            var bs = st.e + stin;
            if (len > ml)
              len = ml;
            for (var i = 0; i < len; ++i) {
              buf[oubt + i] = st.w[bs + i];
            }
            oubt += len, ml -= len, stin = 0;
          }
          for (var i = 0; i < ml; ++i) {
            buf[oubt + i] = buf[stin + i];
          }
          oubt += ml;
        }
        if (oubt != spl) {
          while (spl < buf.length) {
            buf[oubt++] = buf[spl++];
          }
        } else
          oubt = buf.length;
        if (out)
          st.y += oubt;
        else
          buf = slc(buf, 0, oubt);
      } else if (out) {
        st.y += lss;
        if (spl) {
          for (var i = 0; i < lss; ++i) {
            buf[i] = buf[spl + i];
          }
        }
      } else if (spl)
        buf = slc(buf, spl);
      st.b = ebt;
      return buf;
    }
    err(2);
  };
  var cct = function(bufs, ol) {
    if (bufs.length == 1)
      return bufs[0];
    var buf = new u8(ol);
    for (var i = 0, b = 0; i < bufs.length; ++i) {
      var chk = bufs[i];
      buf.set(chk, b);
      b += chk.length;
    }
    return buf;
  };
  function decompress(dat, buf) {
    var bufs = [], nb = +!buf;
    var bt = 0, ol = 0;
    for (; dat.length; ) {
      var st = rzfh(dat, nb || buf);
      if (typeof st == "object") {
        if (nb) {
          buf = null;
          if (st.w.length == st.u) {
            bufs.push(buf = st.w);
            ol += st.u;
          }
        } else {
          bufs.push(buf);
          st.e = 0;
        }
        for (; !st.l; ) {
          var blk = rzb(dat, st, buf);
          if (!blk)
            err(5);
          if (buf)
            st.e = st.y;
          else {
            bufs.push(blk);
            ol += blk.length;
            cpw(st.w, 0, blk.length);
            st.w.set(blk, st.w.length - blk.length);
          }
        }
        bt = st.b + st.c * 4;
      } else
        bt = st;
      dat = dat.subarray(bt);
    }
    return cct(bufs, ol);
  }

  // src/decoder.js
  var MAGIC = [66, 73, 67, 50];
  var VERSION = 2;
  var HEADER_LEN = 32;
  var FLAG_ICC = 1;
  var MAX_ICC = 16 << 20;
  var DEFAULT_MAX_PIXELS = 1e8;
  var MAX_BLOCK = 4096;
  var RLE_BITS = [4, 8, 12, 16];
  var CHANNEL_ORDER = [1, 0, 2, 3];
  var STORAGE_NATIVE = 0;
  var STORAGE_PNG = 1;
  var STORAGE_ORIGINAL = 2;
  var TRANSFORM_DIRECT = 0;
  var TRANSFORM_SUBTRACT_GREEN = 1;
  var TRANSFORM_PALETTE = 2;
  var PREDICTOR_NONE = 0;
  var PREDICTOR_UP = 1;
  var PREDICTOR_PER_ROW = 2;
  var RULE_NONE = 0;
  var RULE_LEFT = 1;
  var RULE_UP = 2;
  var RULE_AVERAGE = 3;
  var RULE_PAETH = 4;
  var RULE_MED = 5;
  var CHANNEL_CONSTANT = 0;
  var CHANNEL_CODED = 1;
  var BicError = class extends Error {
    constructor(message) {
      super(message);
      this.name = "BicError";
    }
  };
  function fail(message) {
    throw new BicError(message);
  }
  function clamp(v, lo, hi) {
    return v < lo ? lo : v > hi ? hi : v;
  }
  function u64(view, offset) {
    const lo = view.getUint32(offset, true);
    const hi = view.getUint32(offset + 4, true);
    if (hi >= 2097152) fail("length exceeds 2^53");
    return hi * 4294967296 + lo;
  }
  function checkedPixels(width, height, maxPixels) {
    const pixels = width * height;
    if (!Number.isSafeInteger(pixels) || pixels > maxPixels) fail("image exceeds pixel limit");
    return pixels;
  }
  function parseContainer(input, { maxPixels = DEFAULT_MAX_PIXELS } = {}) {
    const bytes = input instanceof Uint8Array ? input : new Uint8Array(input);
    if (bytes.length < HEADER_LEN || MAGIC.some((b, i) => bytes[i] !== b)) fail("not a BIC2 file");
    if (bytes[4] !== VERSION) fail("unsupported BIC version");
    const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
    const storage = bytes[5];
    const flags = view.getUint16(6, true);
    if (flags & ~FLAG_ICC) fail("unsupported BIC flags");
    const width = view.getUint32(8, true);
    const height = view.getUint32(12, true);
    const pixels = checkedPixels(width, height, maxPixels);
    const plainLen = u64(view, 16);
    const payloadLen = u64(view, 24);
    let at = HEADER_LEN;
    let icc = null;
    if (flags & FLAG_ICC) {
      if (bytes.length < at + 4) fail("truncated ICC profile");
      const len = view.getUint32(at, true);
      at += 4;
      if (len > MAX_ICC) fail("ICC profile too large");
      if (bytes.length < at + len) fail("truncated ICC profile");
      icc = bytes.subarray(at, at + len);
      at += len;
    }
    if (bytes.length - at !== payloadLen) fail("container payload length mismatch");
    return { storage, width, height, pixels, plainLen, icc, payload: bytes.subarray(at) };
  }
  function sniffImageType(bytes) {
    if (bytes.length >= 8 && bytes[0] === 137 && bytes[1] === 80 && bytes[2] === 78 && bytes[3] === 71) {
      return "image/png";
    }
    if (bytes.length >= 3 && bytes[0] === 255 && bytes[1] === 216 && bytes[2] === 255) return "image/jpeg";
    if (bytes.length >= 12 && String.fromCharCode(...bytes.subarray(0, 4)) === "RIFF" && String.fromCharCode(...bytes.subarray(8, 12)) === "WEBP") {
      return "image/webp";
    }
    if (bytes.length >= 12 && String.fromCharCode(...bytes.subarray(4, 8)) === "ftyp") {
      const brand = String.fromCharCode(...bytes.subarray(8, 12));
      if (["heic", "heix", "heim", "heis", "hevc", "hevx", "mif1", "msf1", "heif"].includes(brand)) {
        return "image/heif";
      }
    }
    return null;
  }
  function quantizer(maxError) {
    const step = 2 * maxError + 1;
    return { near: maxError, step, range: Math.floor((255 + 2 * maxError) / step) + 1 };
  }
  function signed(q, symbol) {
    return symbol >= Math.floor((q.range + 1) / 2) ? symbol - q.range : symbol;
  }
  function reconstruct(q, prediction, symbol) {
    let r = prediction + signed(q, symbol) * q.step;
    if (r < -q.near) r += q.range * q.step;
    else if (r > 255 + q.near) r -= q.range * q.step;
    return clamp(r, 0, 255);
  }
  function predictDelta(rule, left, up, upLeft) {
    switch (rule) {
      case RULE_NONE:
        return 0;
      case RULE_LEFT:
        return left;
      case RULE_UP:
        return up;
      case RULE_AVERAGE:
        return left + up >> 1;
      case RULE_PAETH: {
        const pl = Math.abs(up - upLeft);
        const pu = Math.abs(left - upLeft);
        const pul = Math.abs(left + up - 2 * upLeft);
        if (pl <= pu && pl <= pul) return left;
        return pu <= pul ? up : upLeft;
      }
      case RULE_MED: {
        const lo = Math.min(left, up);
        const hi = Math.max(left, up);
        return clamp(left + up - upLeft, lo, hi);
      }
      default:
        fail("invalid row predictor");
    }
    return 0;
  }
  function reconstructRow(plane, base, width, y, rule, q) {
    const row = y * width;
    const lossless = q.near === 0;
    for (let x = 0; x < width; x++) {
      const i = row + x;
      const b = base === null ? 0 : base[i];
      let p;
      if (rule === RULE_NONE || i === 0) {
        p = clamp(b, 0, 255);
      } else if (y === 0) {
        const left = plane[i - 1] - (base === null ? 0 : base[i - 1]);
        p = clamp(b + left, 0, 255);
      } else if (x === 0) {
        const up = plane[i - width] - (base === null ? 0 : base[i - width]);
        p = clamp(b + up, 0, 255);
      } else {
        const left = plane[i - 1] - (base === null ? 0 : base[i - 1]);
        const up = plane[i - width] - (base === null ? 0 : base[i - width]);
        const upLeft = plane[i - width - 1] - (base === null ? 0 : base[i - width - 1]);
        p = clamp(b + predictDelta(rule, left, up, upLeft), 0, 255);
      }
      const s = plane[i];
      if (lossless) {
        plane[i] = p + s & 255;
      } else {
        if (s >= q.range) fail("symbol outside quantizer range");
        plane[i] = reconstruct(q, p, s);
      }
    }
  }
  function take(data, cursor, n) {
    if (cursor.at + n > data.length) fail("truncated stream");
    const out = data.subarray(cursor.at, cursor.at + n);
    cursor.at += n;
    return out;
  }
  function readU16(data, cursor) {
    const b = take(data, cursor, 2);
    return b[0] | b[1] << 8;
  }
  function readU32(data, cursor) {
    const b = take(data, cursor, 4);
    return (b[0] | b[1] << 8 | b[2] << 16 | b[3] << 24) >>> 0;
  }
  function decodeRle(data, len, bits, out, n) {
    if (data.length < 2) fail("truncated RLE header");
    const runs = data[0] | data[1] << 8;
    if (data.length < 2 + runs) fail("truncated RLE values");
    let at = 2 + runs;
    let buf = 0;
    let have = 0;
    const target = n + len;
    const getCounter = () => {
      while (have < bits) {
        if (at >= data.length) fail("truncated RLE counter");
        buf |= data[at] << have;
        at += 1;
        have += 8;
      }
      const v = buf & (1 << bits) - 1;
      buf >>>= bits;
      have -= bits;
      return v;
    };
    for (let k = 0; k < runs; k++) {
      const value = data[2 + k];
      const count = getCounter() + 1;
      if (n + count > target) fail("RLE overrun");
      out.fill(value, n, n + count);
      n += count;
    }
    if (n !== target) fail("invalid RLE length or padding");
    if (buf !== 0) fail("invalid RLE length or padding");
    for (let k = at; k < data.length; k++) {
      if (data[k] !== 0) fail("invalid RLE length or padding");
    }
    return n;
  }
  function readPlane(data, cursor, expected) {
    const count = readU32(data, cursor);
    if (count > expected) fail("too many blocks");
    const out = new Uint8Array(expected);
    let n = 0;
    for (let k = 0; k < count; k++) {
      const len = readU16(data, cursor);
      if (!(len > 0 && len <= MAX_BLOCK && n + len <= expected)) fail("bad block length");
      const mode = take(data, cursor, 1)[0];
      const bytes = readU32(data, cursor);
      const payload = take(data, cursor, bytes);
      if (mode === 0) {
        if (payload.length !== len) fail("RAW length mismatch");
        out.set(payload, n);
        n += len;
      } else if (mode >= 1 && mode <= 4) {
        n = decodeRle(payload, len, RLE_BITS[mode - 1], out, n);
      } else {
        fail("unknown block mode");
      }
    }
    if (n !== expected) fail("plane length mismatch");
    return out;
  }
  function readChannel(data, cursor, pixels, base, width, predictor, q) {
    const kind = take(data, cursor, 1)[0];
    if (kind === CHANNEL_CONSTANT) {
      const value = take(data, cursor, 1)[0];
      return new Uint8Array(pixels).fill(value);
    }
    if (kind !== CHANNEL_CODED) fail(`invalid channel kind ${kind}`);
    const rows = width === 0 ? 0 : Math.floor(pixels / width);
    let rules;
    if (predictor === PREDICTOR_NONE) {
      rules = new Uint8Array(rows);
    } else if (predictor === PREDICTOR_UP) {
      rules = new Uint8Array(rows).fill(RULE_UP);
    } else {
      rules = readPlane(data, cursor, rows);
      for (const r of rules) if (r > RULE_MED) fail("invalid row predictor");
    }
    const plane = readPlane(data, cursor, pixels);
    for (let y = 0; y < rows; y++) reconstructRow(plane, base, width, y, rules[y], q);
    return plane;
  }
  function decodeNative(width, height, plain) {
    const pixels = width * height;
    const cursor = { at: 0 };
    const header = take(plain, cursor, 3);
    let transform;
    if (header[0] === TRANSFORM_DIRECT) transform = TRANSFORM_DIRECT;
    else if (header[0] === TRANSFORM_SUBTRACT_GREEN) transform = TRANSFORM_SUBTRACT_GREEN;
    else if (header[0] === TRANSFORM_PALETTE) transform = TRANSFORM_PALETTE;
    else fail(`invalid transform ${header[0]}`);
    let predictor;
    if (header[1] === PREDICTOR_NONE) predictor = PREDICTOR_NONE;
    else if (header[1] === PREDICTOR_UP) predictor = PREDICTOR_UP;
    else if (header[1] === PREDICTOR_PER_ROW) predictor = PREDICTOR_PER_ROW;
    else fail(`invalid predictor ${header[1]}`);
    const maxError = header[2];
    const q = quantizer(maxError);
    const rgba = new Uint8Array(pixels * 4);
    if (transform === TRANSFORM_PALETTE) {
      if (maxError !== 0) fail("palette encoding is lossless only");
      const count = take(plain, cursor, 1)[0] + 1;
      const colors = take(plain, cursor, count * 4);
      const indices = readChannel(plain, cursor, pixels, null, width, predictor, q);
      for (let i = 0; i < pixels; i++) {
        const index = indices[i];
        if (index >= count) fail("palette index out of range");
        rgba.set(colors.subarray(index * 4, index * 4 + 4), i * 4);
      }
    } else {
      let green = null;
      for (const c of CHANNEL_ORDER) {
        const base = transform === TRANSFORM_SUBTRACT_GREEN && (c === 0 || c === 2) ? green : null;
        const plane = readChannel(plain, cursor, pixels, base, width, predictor, q);
        for (let i = 0; i < pixels; i++) rgba[i * 4 + c] = plane[i];
        if (c === 1 && transform === TRANSFORM_SUBTRACT_GREEN) green = plane;
      }
    }
    if (cursor.at !== plain.length) fail("trailing native stream bytes");
    return rgba;
  }
  function decodeBic(input, options = {}) {
    const c = parseContainer(input, options);
    if (c.storage === STORAGE_PNG) {
      return { kind: "file", mime: "image/png", bytes: c.payload, width: c.width, height: c.height, icc: c.icc };
    }
    if (c.storage === STORAGE_ORIGINAL) {
      const mime = sniffImageType(c.payload);
      if (!mime) fail("embedded file is not PNG, JPEG, WebP or HEIF");
      return { kind: "file", mime, bytes: c.payload, width: c.width, height: c.height, icc: c.icc };
    }
    if (c.storage !== STORAGE_NATIVE) fail(`unsupported storage type ${c.storage}`);
    const plainLen = c.plainLen;
    if (plainLen > 8 * c.pixels + 4096) fail("stream length does not fit the image size");
    const buffer = new Uint8Array(plainLen);
    let plain;
    try {
      plain = decompress(c.payload, buffer);
    } catch (error) {
      fail(`invalid Zstandard data: ${error.message}`);
    }
    if (plain.length !== plainLen) fail("Zstd output length mismatch");
    const rgba = decodeNative(c.width, c.height, plain);
    return { kind: "pixels", width: c.width, height: c.height, rgba, icc: c.icc };
  }

  // ../../spike/sw-demo/sw-src.js
  async function report(info) {
    const clients = await self.clients.matchAll({ type: "window" });
    for (const client of clients) client.postMessage({ source: "sw", ...info });
  }
  async function serveBic(request) {
    const response = await fetch(request);
    const bytes = new Uint8Array(await response.arrayBuffer());
    const decoded = decodeBic(bytes);
    if (decoded.kind === "file") {
      await report({ url: request.url, network: bytes.length, served: decoded.bytes.length, mode: "original" });
      return new Response(decoded.bytes, { headers: { "Content-Type": decoded.mime } });
    }
    const canvas = new OffscreenCanvas(decoded.width, decoded.height);
    const imageData = new ImageData(new Uint8ClampedArray(decoded.rgba.buffer), decoded.width, decoded.height);
    canvas.getContext("2d").putImageData(imageData, 0, 0);
    const png = await canvas.convertToBlob({ type: "image/png" });
    await report({ url: request.url, network: bytes.length, served: png.size, mode: "decoded" });
    return new Response(png, { headers: { "Content-Type": "image/png" } });
  }
  self.addEventListener("install", () => self.skipWaiting());
  self.addEventListener("activate", (event) => event.waitUntil(self.clients.claim()));
  self.addEventListener("fetch", (event) => {
    const url = new URL(event.request.url);
    if (url.origin !== self.location.origin || !url.pathname.endsWith(".bic")) return;
    event.respondWith(
      serveBic(event.request).catch((error) => {
        report({ url: event.request.url, error: String(error) });
        return Response.error();
      })
    );
  });
})();
