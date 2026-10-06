#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""Generate the snowflake-rust README diagrams.

移植自 snowflake-php 的 scripts/generate-diagrams.py：原语与排版度量原样保留，
架构图/功能图/周期图的文案与结构按 Rust 现实重写，另加一张“请求周期”图。

Reads every label table in scripts/i18n/labels.<lang>.json and writes
docs/i18n/img/<lang>/{architecture,features,lifecycle,request-cycle}.svg

    python3 scripts/generate-diagrams.py            # all languages
    python3 scripts/generate-diagrams.py en zh-CN   # selected languages

Verify a change by rasterizing and looking at the result:

    rsvg-convert -b white docs/i18n/img/zh-CN/architecture.svg -o /tmp/a.png

Notes for editors:
  * SVG collapses runs of whitespace inside <text>, so never rely on double
    spaces for alignment - use a separator like " · " instead.
  * Labels that are too wide for their box are auto-shrunk by fit(), but long
    translations still read better when they are short. Keep them short.
"""
import glob
import html
import json
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
LABEL_DIR = os.path.join(ROOT, "scripts", "i18n")
OUT_DIR = os.path.join(ROOT, "docs", "i18n", "img")

FONT = ("-apple-system, BlinkMacSystemFont, 'Segoe UI', 'Noto Sans', "
        "'PingFang SC', 'Hiragino Sans', 'Yu Gothic', 'Malgun Gothic', "
        "Roboto, 'Noto Sans CJK SC', 'Source Han Sans SC', 'Microsoft YaHei', "
        "'Kohinoor Devanagari', 'Devanagari Sangam MN', 'Nirmala UI', "
        "'Noto Sans Devanagari', 'Bangla MN', 'Bangla Sangam MN', 'Noto Sans Bengali', "
        "'Geeza Pro', 'Noto Sans Arabic', Arial, sans-serif")
MONO = ("'SFMono-Regular', Consolas, 'Liberation Mono', Menlo, "
        "'Courier New', monospace")

INK   = "#0f1e33"; BODY = "#3d4d66"; MUTED = "#6b7c96"; FAINT = "#93a3b8"
LINE  = "#dbe6f5"; RULE = "#e8eefa"
BLUE  = "#2563eb"; BLUE_T = "#eef4ff"; BLUE_B = "#c3d7ff"
CYAN  = "#0e7490"; CYAN_T = "#e8f7fb"; CYAN_B = "#a9dcea"
VIOLET = "#6d28d9"; VIOLET_T = "#f4efff"; VIOLET_B = "#d5c6fa"
AMBER = "#b45309"; AMBER_T = "#fff6e6"; AMBER_B = "#f7d59b"
RED   = "#dc2626"; RED_T  = "#fdeeee"; RED_B   = "#f4c2c2"
GREEN = "#0f766e"; GREEN_T = "#e9f7f3"; GREEN_B = "#a9dcd0"
SLATE = "#475569"; SLATE_T = "#f4f7fb"; SLATE_B = "#dbe3ec"
ARROW = "#93a6bf"; ARROW_HI = "#5b8def"

BADGE = "snowflake-rust"
COPYRIGHT = "© 2026 erik — https://erik.xyz"
FOOTER = 46


# ---------------------------------------------------------------- text metrics

def is_cjk(ch):
    return ord(ch) >= 0x2E80


def tw(s, size, weight="400"):
    """Rough advance width in px. Deliberately conservative: over-estimating
    only costs a little padding, under-estimating overflows a box."""
    w = 0.0
    for ch in s:
        o = ord(ch)
        if o >= 0x2E80:                      # CJK, kana, hangul: full width
            w += size * 1.0
        elif 0x0900 <= o <= 0x097F or 0x0980 <= o <= 0x09FF:
            w += size * 0.66                 # Devanagari / Bengali
        elif 0x0600 <= o <= 0x06FF or 0x0750 <= o <= 0x077F:
            w += size * 0.52                 # Arabic
        elif 0x0370 <= o <= 0x04FF:
            w += size * 0.56                 # Greek / Cyrillic
        elif ch in "iljItf.,:;'|!()[]":
            w += size * 0.32
        elif ch in "mMW@":
            w += size * 0.86
        elif ch.isupper() or ch.isdigit():
            w += size * 0.62
        else:
            w += size * 0.53
    if weight in ("600", "700"):
        w *= 1.05
    return w


def fit(s, size, maxw, weight="400"):
    """Shrink the font size just enough for a single-line label to fit."""
    w = tw(s, size, weight)
    if w <= maxw:
        return size
    return max(7.5, size * maxw / w)


def wrap(s, size, maxw, weight="400"):
    """Greedy wrap. CJK breaks anywhere; space-separated scripts stay whole."""
    units, buf = [], ""
    for ch in s:
        if is_cjk(ch) or ch == " ":
            if buf:
                units.append(buf)
                buf = ""
            units.append(ch)
        else:
            buf += ch
    if buf:
        units.append(buf)

    lines, cur = [], ""
    for u in units:
        if u == " ":
            if cur and not cur.endswith(" "):
                cur += " "
            continue
        if cur and not cur.endswith(" ") and not is_cjk(cur[-1]) and not is_cjk(u[0]):
            trial = cur + " " + u
        else:
            trial = cur + u
        if tw(trial.rstrip(), size, weight) <= maxw:
            cur = trial
        else:
            if cur.strip():
                lines.append(cur.strip())
            cur = u
    if cur.strip():
        lines.append(cur.strip())
    return lines


# ------------------------------------------------------------------ primitives

def esc(s):
    return html.escape(str(s), quote=True)


def text(x, y, s, size=13, fill=BODY, weight="400", anchor="start",
         family=None, opacity=None, spacing=None):
    a = f' text-anchor="{anchor}"' if anchor != "start" else ""
    f = f' font-family="{family}"' if family else ""
    o = f' opacity="{opacity}"' if opacity else ""
    sp = f' letter-spacing="{spacing}"' if spacing else ""
    return (f'<text x="{x}" y="{y}" font-size="{size:.1f}" fill="{fill}" '
            f'font-weight="{weight}"{a}{f}{o}{sp}>{esc(s)}</text>')


def block(x, y, lines, size=11.5, fill=BODY, weight="400", anchor="start", lh=None):
    lh = lh if lh else size * 1.5
    return "\n".join(text(x, y + i * lh, l, size, fill, weight, anchor)
                     for i, l in enumerate(lines))


def tline(x, y, parts, size=12, anchor="start"):
    """Inline runs: [(text, fill, weight)]"""
    tspans = "".join(f'<tspan fill="{c}" font-weight="{w}">{esc(t)}</tspan>'
                     for t, c, w in parts)
    return (f'<text x="{x}" y="{y}" font-size="{size}" fill="{BODY}" '
            f'text-anchor="{anchor}">{tspans}</text>')


def rect(x, y, w, h, fill="#fff", stroke=None, rx=10, sw=1.5, dash=None):
    s = f' stroke="{stroke}" stroke-width="{sw}"' if stroke else ""
    d = f' stroke-dasharray="{dash}"' if dash else ""
    return (f'<rect x="{x:.1f}" y="{y:.1f}" width="{w:.1f}" height="{h:.1f}" rx="{rx}" '
            f'fill="{fill}"{s}{d}/>')


def line(x1, y1, x2, y2, color=ARROW, sw=2, dash=None, cap="round"):
    d = f' stroke-dasharray="{dash}"' if dash else ""
    return (f'<line x1="{x1:.1f}" y1="{y1:.1f}" x2="{x2:.1f}" y2="{y2:.1f}" '
            f'stroke="{color}" stroke-width="{sw}" stroke-linecap="{cap}"{d}/>')


def arrow(x1, y1, x2, y2, color=ARROW, sw=2, dash=None, head=None):
    m = f' marker-end="url(#mk-{(head or color.lstrip("#"))})"'
    da = f' stroke-dasharray="{dash}"' if dash else ""
    return (f'<path d="M{x1:.1f},{y1:.1f} L{x2:.1f},{y2:.1f}" fill="none" '
            f'stroke="{color}" stroke-width="{sw}" stroke-linecap="round"{da}{m}/>')


def diamond(cx, cy, w, h, fill="#fff", stroke=BLUE, sw=1.8):
    pts = f"{cx},{cy - h / 2} {cx + w / 2},{cy} {cx},{cy + h / 2} {cx - w / 2},{cy}"
    return f'<polygon points="{pts}" fill="{fill}" stroke="{stroke}" stroke-width="{sw}"/>'


def pill(x, y, w, h, label, fill, size=12, tcolor="#fff", weight="600", maxw=None):
    if maxw and w > maxw:
        x += (w - maxw) / 2
        w = maxw
    size = fit(label, size, w - 16, weight)
    return (rect(x, y, w, h, fill, None, h / 2)
            + text(x + w / 2, y + h / 2 + size * 0.36, label, size, tcolor,
                   weight, "middle"))


def card(x, y, w, h, title, body_lines, accent=BLUE, tint="#fff", border=None,
         tsize=13, bsize=11.5, tag=None, tagfill=None, pad=15):
    border = border or LINE
    out = [rect(x, y, w, h, tint, border, 12),
           rect(x, y, 4, h, accent, None, 2),
           text(x + pad, y + 24, title, fit(title, tsize, w - pad * 2, "700"),
                INK, "700")]
    ly = y + 46
    if tag:
        twd = tw(tag, 10.5, "600") + 16
        out.append(rect(x + pad, y + 32, twd, 18, tagfill or BLUE_T, None, 9))
        out.append(text(x + pad + twd / 2, y + 45, tag, 10.5, accent, "600", "middle"))
        ly = y + 66
    if body_lines:
        lines = []
        for para in body_lines:
            lines.extend(wrap(para, bsize, w - pad * 2))
        out.append(block(x + pad, ly, lines, bsize, BODY))
    return "\n".join(out)


def defs(body):
    used = sorted(set(re.findall(r'url\(#mk-([0-9a-fA-F]{6})\)', body)))
    out = ['<defs>']
    for cid in used:
        out.append(
            f'<marker id="mk-{cid}" viewBox="0 0 10 10" refX="8.5" refY="5" '
            f'markerWidth="6.5" markerHeight="6.5" orient="auto">'
            f'<path d="M0,0.6 L9.5,5 L0,9.4 z" fill="#{cid}"/></marker>')
    out.append('</defs>')
    return "\n".join(out)


def svg(w, h, body, title, desc):
    """Wrap the body, then append the copyright footer below the content."""
    fh = h + FOOTER
    foot = (text(40, h + 28, COPYRIGHT, 11.5, FAINT)
            + text(w - 40, h + 28, BADGE, 11.5, FAINT, "600", "end"))
    return (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {fh}" '
            f'width="{w}" height="{fh}" role="img" font-family="{FONT}">\n'
            f'<title>{esc(title)}</title>\n<desc>{esc(desc)}</desc>\n'
            f'{defs(body)}\n<rect width="{w}" height="{fh}" fill="#ffffff"/>\n'
            f'{body}\n{foot}\n</svg>\n')


def mini_snowflake(cx, cy, r, color=BLUE, sw=1.6):
    """项目宠物“雪花精灵”的迷你版：六分支 + 雪核，用于徽章图标位。

    取自 pet.svg 的同一几何：六条主臂、每臂两片小叉、中心雪核。
    """
    out = [f'<circle cx="{cx:.1f}" cy="{cy:.1f}" r="{r * 0.24:.1f}" fill="{color}"/>']
    for i in range(6):
        out.append(
            f'<g transform="rotate({i * 60} {cx:.1f} {cy:.1f})">'
            f'<line x1="{cx + r * 0.32:.1f}" y1="{cy:.1f}" x2="{cx + r:.1f}" y2="{cy:.1f}" '
            f'stroke="{color}" stroke-width="{sw}" stroke-linecap="round"/>'
            f'<line x1="{cx + r * 0.62:.1f}" y1="{cy:.1f}" '
            f'x2="{cx + r * 0.88:.1f}" y2="{cy - r * 0.25:.1f}" '
            f'stroke="{color}" stroke-width="{sw * 0.75:.2f}" stroke-linecap="round"/>'
            f'<line x1="{cx + r * 0.62:.1f}" y1="{cy:.1f}" '
            f'x2="{cx + r * 0.88:.1f}" y2="{cy + r * 0.25:.1f}" '
            f'stroke="{color}" stroke-width="{sw * 0.75:.2f}" stroke-linecap="round"/>'
            f'</g>')
    return "".join(out)


def header(w, title, sub, badge=BADGE):
    out = [text(40, 54, title, fit(title, 27, w - 300, "700"), INK, "700"),
           text(40, 82, sub, fit(sub, 13.5, w - 300), MUTED),
           line(40, 100, w - 40, 100, RULE, 1.5, cap="butt")]
    # 徽章图标位放项目宠物（迷你雪花精灵），文字跟在右侧。
    twid = tw(badge, 11.5, "600")
    bw = twid + 42
    bx = w - 40 - bw
    out.append(rect(bx, 36, bw, 26, BLUE_T, BLUE_B, 13, 1.2))
    out.append(mini_snowflake(bx + 15, 49, 8))
    out.append(text(bx + 26 + twid / 2, 53, badge, 11.5, BLUE, "600", "middle"))
    return "\n".join(out)


# --------------------------------------------------------------- architecture

def architecture(L):
    """Layered view: application → integrations → core → contracts, plus the
    cross-cutting pieces.

    Every shipped integration gets a slot, and the bands lay their slots out
    in a centred grid, so adding a framework means adding labels - not
    coordinates. All vertical positions derive from the band heights, which
    derive from the number of slots.
    """
    W = 980
    COLS, GAP = 4, 16                     # slots per row inside a band
    BW = (W - 112 - (COLS - 1) * GAP) / COLS
    X0 = 64
    PITCH = 64                            # row pitch inside a band
    APP_H, ADP_H, CORE_H, CORE_PITCH = 52, 64, 58, 74
    TOP, AFTER_ARROW = 116, 28            # first band top, gap after an arrow

    def rows(n):
        return (n + COLS - 1) // COLS

    def slots(items, y0, box_h):
        """Yield (key-tuple, x, y) for each slot, later rows centred."""
        out = []
        total = rows(len(items))
        for i, item in enumerate(items):
            row, col = divmod(i, COLS)
            in_row = min(COLS, len(items) - row * COLS)
            offset = (COLS - in_row) * (BW + GAP) / 2
            out.append((item, X0 + offset + col * (BW + GAP), y0 + row * PITCH))
        return out, total * PITCH - (PITCH - box_h) + 48   # band height

    body = [header(W, L("arch.title"), L("arch.subtitle"))]

    def band(y, h, key, color, tint):
        label = L(key)
        out = [rect(40, y, W - 80, h, tint, LINE, 14, 1.4)]
        lw = min(tw(label, 12, "600") + 30, 340)
        out.append(pill(58, y - 13, lw, 26, label, color, 12))
        return "\n".join(out)

    # 八个框架 + 原生 Rust：应用侧先按接入顺序排，原生收尾。
    apps = [("arch.app1", "arch.app1.sub"), ("arch.app2", "arch.app2.sub"),
            ("arch.app3", "arch.app3.sub"), ("arch.app4", "arch.app4.sub"),
            ("arch.app5", "arch.app5.sub"), ("arch.app6", "arch.app6.sub"),
            ("arch.app7", "arch.app7.sub"), ("arch.app8", "arch.app8.sub"),
            ("arch.app9", "arch.app9.sub")]
    app_slots, app_h = slots(apps, TOP + 26, APP_H)
    body.append(band(TOP, app_h, "arch.band.app", BLUE, "#f8fbff"))
    for (k, ks), x, y in app_slots:
        body.append(rect(x, y, BW, APP_H, "#ffffff", BLUE_B, 10))
        body.append(text(x + BW / 2, y + 23, L(k), fit(L(k), 13, BW - 16, "600"), INK, "600", "middle"))
        body.append(text(x + BW / 2, y + 41, L(ks), fit(L(ks), 11, BW - 16), MUTED, "400", "middle"))

    adp = [("arch.ad1", "arch.ad1.sub1", "arch.ad1.sub2"), ("arch.ad2", "arch.ad2.sub1", "arch.ad2.sub2"),
           ("arch.ad3", "arch.ad3.sub1", "arch.ad3.sub2"), ("arch.ad4", "arch.ad4.sub1", "arch.ad4.sub2"),
           ("arch.ad5", "arch.ad5.sub1", "arch.ad5.sub2"), ("arch.ad6", "arch.ad6.sub1", "arch.ad6.sub2"),
           ("arch.ad7", "arch.ad7.sub1", "arch.ad7.sub2"), ("arch.ad8", "arch.ad8.sub1", "arch.ad8.sub2")]
    adp_y = TOP + app_h + AFTER_ARROW
    body.append(arrow(490, TOP + app_h, 490, TOP + app_h + 22, ARROW_HI, 2, head="5b8def"))
    body.append(text(500, TOP + app_h + 16, L("arch.arrow.instance"), fit(L("arch.arrow.instance"), 11, 400), MUTED))
    adp_slots, adp_h = slots(adp, adp_y + 26, ADP_H)
    body.append(band(adp_y, adp_h, "arch.band.integration", BLUE, "#f8fbff"))
    for (k, k1, k2), x, y in adp_slots:
        body.append(rect(x, y, BW, ADP_H, "#ffffff", BLUE_B, 10))
        body.append(text(x + 14, y + 21, L(k), fit(L(k), 12.5, BW - 24, "700"), BLUE, "700"))
        body.append(text(x + 14, y + 39, L(k1), fit(L(k1), 10.5, BW - 24), BODY))
        body.append(text(x + 14, y + 55, L(k2), fit(L(k2), 10.5, BW - 24), MUTED))

    core_y = adp_y + adp_h + AFTER_ARROW
    body.append(arrow(490, adp_y + adp_h, 490, adp_y + adp_h + 22, ARROW_HI, 2, head="5b8def"))
    body.append(text(500, adp_y + adp_h + 16, L("arch.arrow.config"), 11, MUTED, family=MONO))

    core = [("arch.core1", "arch.core1.sub"), ("arch.core2", "arch.core2.sub"),
            ("arch.core3", "arch.core3.sub"), ("arch.core4", "arch.core4.sub"),
            ("arch.core5", "arch.core5.sub"), ("arch.core6", "arch.core6.sub")]
    core_rows = rows(len(core))
    # First row sits below the band title (44), not at the band's 26px inset.
    core_h = 44 + (core_rows - 1) * CORE_PITCH + CORE_H + 16
    body.append(band(core_y, core_h, "arch.band.core", BLUE, "#f3f8ff"))
    body.append(text(64, core_y + 30, L("arch.core.title"), fit(L("arch.core.title"), 12.5, W - 200, "700"),
                     INK, "700"))
    cw = (W - 80 - 48 - 32) / 3
    for i, (k, ks) in enumerate(core):
        x = 64 + (i % 3) * (cw + 16)
        y = core_y + 44 + (i // 3) * CORE_PITCH
        body.append(rect(x, y, cw, CORE_H, "#ffffff", BLUE_B, 10))
        body.append(text(x + 14, y + 25, L(k), fit(L(k), 12.5, cw - 24, "600"), INK, "600"))
        body.append(text(x + 14, y + 45, L(ks), fit(L(ks), 10.5, cw - 24), MUTED))

    iface_y = core_y + core_h + AFTER_ARROW
    body.append(arrow(490, core_y + core_h, 490, core_y + core_h + 22, ARROW_HI, 2, head="5b8def"))
    body.append(text(500, core_y + core_h + 16, L("arch.arrow.next"), fit(L("arch.arrow.next"), 11, 400), MUTED))

    contract_h = 180
    body.append(band(iface_y, contract_h, "arch.band.contract", CYAN, "#f6fdfe"))
    body.append(rect(64, iface_y + 24, 300, 140, "#ffffff", CYAN_B, 10))
    body.append(text(80, iface_y + 50, "SequenceResolver", 13, CYAN, "700", family=MONO))
    body.append(text(80, iface_y + 70, L("arch.iface.sub"), fit(L("arch.iface.sub"), 10.5, 268), MUTED))
    sig = ["fn next(&mut self,",
           "    timestamp_ms: i64,",
           "    max_sequence: i64,",
           ") -> Result<Option<i64>>"]
    for i, row in enumerate(sig):
        body.append(text(80, iface_y + 94 + i * 16, row, 10.5, BODY, family=MONO))
    for i, (k, ks) in enumerate([("arch.impl1", "arch.impl1.sub"), ("arch.impl2", "arch.impl2.sub"),
                                 ("arch.impl3", "arch.impl3.sub"), ("arch.impl4", "arch.impl4.sub")]):
        y = iface_y + 24 + i * 36
        body.append(rect(400, y, 540, 32, "#ffffff", CYAN_B, 8))
        body.append(arrow(400, y + 16, 368, y + 16, CYAN, 1.8, head="0e7490"))
        name, sub = L(k), L(ks)
        size = min(fit(name + sub, 11.5, 512, "700"), 11.5)
        body.append(tline(414, y + 21, [(name, CYAN, "700"), (sub, MUTED, "400")], size))
    body.append(text(356, iface_y + 80, L("arch.implements"), 10, FAINT, "600", "end"))

    cross_y = iface_y + contract_h + AFTER_ARROW
    body.append(band(cross_y, 150, "arch.band.cross", SLATE, "#fbfcfe"))
    body.append(text(64, cross_y + 32, L("arch.exc.title"), 12, INK, "700"))
    exs = [("ClockDrift", RED, RED_T),
           ("ClockBeforeEpoch", AMBER, AMBER_T),
           ("TimestampOverflow", AMBER, AMBER_T),
           ("InvalidWorkerId", AMBER, AMBER_T),
           ("InvalidDatacenterId", AMBER, AMBER_T),
           ("InvalidConfig", SLATE, SLATE_T),
           ("SequenceUnavailable", SLATE, SLATE_T),
           ("Backend", RED, RED_T)]
    px, py = 64, cross_y + 44
    for name, c, t in exs:
        w = tw(name, 10.5, "600") + 22
        if px + w > 560:
            px, py = 64, py + 26
        body.append(rect(px, py, w, 22, t, None, 11))
        body.append(text(px + w / 2, py + 15, name, 10.5, c, "600", "middle"))
        px += w + 8
    body.append(line(608, cross_y + 24, 608, cross_y + 130, LINE, 1.5, cap="butt"))
    body.append(text(632, cross_y + 32, L("arch.cfg.title"), 12, INK, "700"))
    cfg = []
    for k in ("arch.cfg1", "arch.cfg2", "arch.cfg3"):
        cfg.extend(wrap(L(k), 11, 288))
    body.append(block(632, cross_y + 54, cfg, 11 if len(cfg) <= 4 else 10, BODY, lh=15))

    H = cross_y + 150 + 40
    return svg(W, H, "\n".join(body), L("arch.title"), L("arch.subtitle"))


# ------------------------------------------------------------------- features

def features(L):
    W, H = 980, 820
    body = [header(W, L("feat.title"), L("feat.subtitle"))]
    groups = [
        ("feat.g1", BLUE, "#f8fbff", BLUE_B, [1, 2, 3]),
        ("feat.g2", CYAN, "#f6fdfe", CYAN_B, [4, 5, 6]),
        ("feat.g3", VIOLET, "#fbf9ff", VIOLET_B, [7, 8, 9]),
    ]
    pw = (W - 80 - 40) / 3
    for gi, (gkey, color, tint, border, cards) in enumerate(groups):
        x = 40 + gi * (pw + 20)
        name = L(gkey)
        body.append(rect(x, 120, pw, 652, tint, LINE, 14, 1.4))
        body.append(pill(x + 16, 136, min(tw(name, 12.5, "700") + 28, pw - 32), 28,
                         name, color, 12.5, maxw=pw - 32))
        for ci, n in enumerate(cards):
            y = 184 + ci * 194
            body.append(card(x + 16, y, pw - 32, 180, L(f"feat.c{n}"), [L(f"feat.c{n}.desc")],
                             color, "#ffffff", border, 13, 11.5, L(f"feat.c{n}.tag"), tint))
    body.append(text(40, 796, L("feat.footer"), fit(L("feat.footer"), 11.5, W - 80), MUTED))
    return svg(W, H, "\n".join(body), L("feat.title"), L("feat.subtitle"))


# ------------------------------------------------------------------ lifecycle

def lifecycle(L):
    W, H = 980, 950
    CX, RX, RW = 350, 650, 290
    body = [header(W, L("life.title"), L("life.subtitle"))]

    body.append(card(RX, 120, RW, 186, L("life.bits.title"), None, BLUE, "#fff", BLUE_B))
    segs = [(1, SLATE, ""), (41, BLUE, L("life.bits.seg.timestamp")), (5, VIOLET, ""),
            (5, CYAN, ""), (12, AMBER, "")]
    bx, by, bh = RX + 16, 160, 26
    avail = RW - 40
    for i, (bits, c, lab) in enumerate(segs):
        w = max(9, avail * bits / 64)
        body.append(rect(bx, by, w, bh, c, None, 4 if i in (0, 4) else 0))
        if lab:
            body.append(text(bx + w / 2, by + 18, lab, fit(lab, 10, w - 4, "600"),
                             "#fff", "600", "middle"))
        bx += w
    bits = []
    for k in ("life.bits.l1", "life.bits.l2", "life.bits.l3"):
        bits.extend(wrap(L(k), 10.5, 258))
    body.append(block(RX + 16, 208, bits, 10.5 if len(bits) <= 5 else 9.5, MUTED, lh=15))

    body.append(card(RX, 324, RW, 204, L("life.state.title"),
                     [L("life.state.l1"), L("life.state.l2"), L("life.state.l3")],
                     VIOLET, "#fbf9ff", VIOLET_B))
    note = wrap(L("life.state.note"), 10.5, RW - 40)
    body.append(block(RX + 16, 486, note, 10.5 if len(note) <= 3 else 9.5, RED, "600",
                      lh=(10.5 if len(note) <= 3 else 9.5) * 1.35))

    body.append(card(RX, 546, RW, 162, L("life.cap.title"),
                     [L("life.cap.l1"), L("life.cap.l2"), L("life.cap.l3")],
                     CYAN, "#f6fdfe", CYAN_B))

    body.append(card(RX, 726, RW, 182, L("life.time.title"), None, AMBER, "#fffaf0", AMBER_B))
    tx, ty, twd = RX + 20, 780, RW - 40
    body.append(rect(tx, ty, twd, 10, SLATE_B, None, 5))
    body.append(rect(tx, ty, twd * 0.035, 10, AMBER, None, 5))
    body.append(line(tx + twd * 0.035, ty - 4, tx + twd * 0.035, ty + 15, AMBER, 2, cap="butt"))
    body.append(text(tx, ty - 9, L("life.time.epoch"), fit(L("life.time.epoch"), 10, 120, "600"),
                     AMBER, "600"))
    body.append(text(tx + twd, ty - 9, L("life.time.limit"), fit(L("life.time.limit"), 10, 120, "600"),
                     MUTED, "600", "end"))
    tl = []
    for k in ("life.time.l1", "life.time.l2", "life.time.l3", "life.time.l4"):
        tl.extend(wrap(L(k), 10.5, 248))
    # The block's height depends on how many lines the translation needs;
    # tighten the leading when it runs long so the trailing note stays inside
    # the card instead of landing on its bottom border.
    lh = 16 if len(tl) <= 4 else 14
    body.append(block(tx, ty + 36, tl, 10.5, BODY, lh=lh))
    body.append(tline(tx, ty + 48 + lh * len(tl),
                      [(L("life.time.now"), MUTED, "400")], 10.5))

    state = {"y": 120, "prev": None}

    def flow(label, caption=None, dia=False, sub=None, alabel=None):
        y, prev = state["y"], state["prev"]
        if prev is not None:
            body.append(arrow(CX, prev, CX, y, ARROW_HI, 2, head="5b8def"))
            if alabel:
                body.append(text(CX + 12, (prev + y) / 2 + 4, alabel, 10.5, MUTED, "600"))
        h = 68 if dia else 50
        if dia:
            body.append(diamond(CX, y + h / 2, 300, h, "#fff", BLUE, 1.8))
            lines = 2 if sub else 1
            y0 = y + h / 2 + 5 - (lines - 1) * 8
            body.append(text(CX, y0, label, fit(label, 12.5, 210 if sub else 250, "600"),
                             INK, "600", "middle"))
            if sub:
                body.append(text(CX, y0 + 16, sub, fit(sub, 10.5, 160), MUTED, "400", "middle"))
        else:
            body.append(rect(CX - 170, y, 340, h, "#fff", BLUE_B, 10))
            body.append(text(CX, y + h / 2 + 4.5, label, fit(label, 12.5, 320, "600"),
                             INK, "600", "middle"))
        bottom = y + h
        if caption:
            lines = wrap(caption, 10.5, 344)
            body.append(block(CX - 172, bottom + 19, lines, 10.5, AMBER, lh=16))
            bottom += 19 + 16 * (len(lines) - 1)
        state["y"], state["prev"] = bottom + 24, bottom

    body.append(pill(CX - 110, 120, 220, 36, L("life.flow.start"), GREEN, 12.5))
    state["y"], state["prev"] = 180, 156

    flow(L("life.flow.n1"), L("life.flow.n1.cap"))
    flow(L("life.flow.d1"), L("life.flow.d1.cap"), dia=True, alabel=L("life.flow.no"))
    flow(L("life.flow.n2"), L("life.flow.n2.cap"))
    flow(L("life.flow.d2"), L("life.flow.d2.cap"), dia=True, sub=L("life.flow.d2.sub"),
         alabel=L("life.flow.no"))
    flow(L("life.flow.n3"), L("life.flow.n3.cap"))
    flow(L("life.flow.n4"), L("life.flow.n4.cap"))

    yy = state["y"]
    body.append(arrow(CX, state["prev"], CX, yy, ARROW_HI, 2, head="5b8def"))
    body.append(pill(CX - 170, yy, 340, 38, L("life.flow.end"), GREEN, 12.5))
    return svg(W, H, "\n".join(body), L("life.title"), L("life.subtitle"))


# ------------------------------------------------------------- request cycle

def request_cycle(L):
    """How one web request obtains the guard and burns an ID: registration at
    boot, extraction per request, generation inside the handler.

    Left column is the request path (start → five steps → done); the right
    column carries the four things worth knowing: the eight registration
    shapes, why extraction is free, what a missing registration means, and who
    owns generation failures.
    """
    W, H = 980, 856
    CX, RX, RW = 350, 650, 290
    body = [header(W, L("req.title"), L("req.subtitle"))]

    reg = [L(f"req.reg.l{i}") for i in range(1, 9)]
    body.append(card(RX, 120, RW, 218, L("req.reg.title"), reg, BLUE, "#fff", BLUE_B, 13, 10.5))

    body.append(card(RX, 354, RW, 152, L("req.cost.title"),
                     [L("req.cost.l1"), L("req.cost.l2"), L("req.cost.l3")],
                     GREEN, "#f3fbf8", GREEN_B))
    body.append(card(RX, 522, RW, 136, L("req.miss.title"),
                     [L("req.miss.l1"), L("req.miss.l2")],
                     AMBER, "#fffaf0", AMBER_B))
    body.append(card(RX, 674, RW, 132, L("req.fail.title"),
                     [L("req.fail.l1"), L("req.fail.l2")],
                     RED, "#fdf3f3", RED_B))

    body.append(pill(CX - 130, 120, 260, 36, L("req.flow.start"), GREEN, 12.5))

    y, prev = 180, 156
    for i in range(1, 6):
        label = L(f"req.flow.n{i}")
        body.append(arrow(CX, prev, CX, y, ARROW_HI, 2, head="5b8def"))
        body.append(rect(CX - 170, y, 340, 50, "#fff", BLUE_B, 10))
        body.append(text(CX, y + 29.5, label, fit(label, 12.5, 320, "600"),
                         INK, "600", "middle"))
        prev, y = y + 50, y + 74

    body.append(arrow(CX, prev, CX, y, ARROW_HI, 2, head="5b8def"))
    body.append(pill(CX - 170, y, 340, 38, L("req.flow.end"), GREEN, 12.5))
    return svg(W, H, "\n".join(body), L("req.title"), L("req.subtitle"))


# ------------------------------------------------------------------- driver

def load_labels(lang):
    path = os.path.join(LABEL_DIR, f"labels.{lang}.json")
    with open(path, encoding="utf-8") as f:
        data = json.load(f)
    meta = data.pop("_meta")

    def L(key, _d=data, _lang=lang):
        try:
            return _d[key]
        except KeyError:
            raise SystemExit(f"labels.{_lang}.json is missing key: {key}")

    return meta, L


def main(argv):
    langs = argv[1:] or sorted(
        os.path.basename(p)[len("labels."):-len(".json")]
        for p in glob.glob(os.path.join(LABEL_DIR, "labels.*.json")))

    need = set(re.findall(r'L\("([^"]+)"\)', open(__file__, encoding="utf-8").read()))
    problems, total = [], 0
    for lang in langs:
        meta, L = load_labels(lang)
        missing = need - set(json.load(open(os.path.join(LABEL_DIR, f"labels.{lang}.json"),
                                            encoding="utf-8")))
        if missing:
            problems.append(f"{lang}: {', '.join(sorted(missing))}")
            continue

        out = os.path.join(OUT_DIR, lang)
        os.makedirs(out, exist_ok=True)
        for name, fn in (("architecture", architecture), ("features", features),
                         ("lifecycle", lifecycle), ("request-cycle", request_cycle)):
            with open(os.path.join(out, f"{name}.svg"), "w", encoding="utf-8") as f:
                f.write(fn(L))
            total += 1
        print(f"  {lang:6s} {meta['name']:16s} 4 diagrams")

    if problems:
        raise SystemExit("incomplete label tables:\n  " + "\n  ".join(problems))
    print(f"wrote {total} SVG files to {os.path.relpath(OUT_DIR, ROOT)}")


if __name__ == "__main__":
    main(sys.argv)
