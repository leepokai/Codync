#!/usr/bin/env python3
"""Builds the Dynamic Island's animations as fonts.

Live Activities draw one frame and only a timer Text keeps updating, so each animation is a
timer whose last digit is drawn in a font where `0`–`9` are the ten frames of a loop (every
other character the timer prints shapes to nothing). One font per layer, stacked and tinted in
the widget: the working bot (`CodyncBot-<shape>-Ink` / `-Tint`) and the halftone thinking orb
(`CodyncOrb-Ghost` / `-Dot`).

The frames come from the app's own drawing code (`DottedBody.workingLoop`,
`ThinkingOrbGeometry.workingLoop`):

    cd apps/ios/Kit && TEST_RUNNER_CODYNC_FRAMES_FILE=/tmp/frames.json xcodebuild test \
      -scheme CodyncKit-Package -derivedDataPath ../../../build/dd \
      -destination 'platform=iOS Simulator,id=<sim>' '-only-testing:CodyncKitTests/exportBotFrames()'
    tools/timer-fonts.py /tmp/frames.json apps/ios/Widgets/Resources/Fonts
"""

import json
import math
import subprocess
import sys
from pathlib import Path

from fontTools.fontBuilder import FontBuilder
from fontTools.pens.ttGlyphPen import TTGlyphPen

EM = 1000
DIGITS = ["zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine"]


def circle(pen, cx, cy, r):
    """A clockwise circle of eight off-curve points (TrueType quadratics)."""
    k = r / math.cos(math.pi / 8)
    points = [(cx + k * math.cos(-i * math.pi / 4), cy + k * math.sin(-i * math.pi / 4)) for i in range(8)]
    pen.qCurveTo(*[(round(x), round(y)) for x, y in points], None)
    pen.closePath()


def glyph(dots, size):
    pen = TTGlyphPen(None)
    scale = EM / size
    for d in dots:
        circle(pen, d["x"] * scale, (size - d["y"]) * scale, d["r"] * scale)
    return pen.glyph()


def empty():
    return TTGlyphPen(None).glyph()


# Every character a timer can print besides digits: ASCII and the ratio colon, the narrow and
# no-break spaces and the full-width colon some locales use.
SEPARATORS = {"space": 0x20, "colon": 0x3A, "period": 0x2E, "comma": 0x2C, "ratio": 0x2236,
              "nbsp": 0xA0, "nnbsp": 0x202F, "thinsp": 0x2009, "fwcolon": 0xFF1A}
# Only the last digit draws: a digit followed by anything the timer prints becomes `blank`
# (contextual alternates, on by default), so the whole timer is exactly one face wide and
# nothing depends on how the system aligns or clips timer text.
FEATURES = """
@DIGIT = [zero one two three four five six seven eight nine];
@NEXT = [@DIGIT space colon period comma ratio nbsp nnbsp thinsp fwcolon];
feature calt {
    sub @DIGIT' @NEXT by blank;
} calt;
"""


def names(name):
    """The name records App Store font validation requires (family, style, unique, full,
    version, PostScript); without the full name and version it rejects the build (ITMS-90853)."""
    family = name.replace("-", " ")
    return {"familyName": family, "styleName": "Regular", "uniqueFontIdentifier": f"Codync: {name}",
            "fullName": family, "version": "Version 1.000", "psName": name}


def build(name, frames, size, out):
    order = [".notdef", "blank", *SEPARATORS, *DIGITS]
    glyphs = {g: empty() for g in [".notdef", "blank", *SEPARATORS]}
    for digit, dots in zip(DIGITS, frames):
        glyphs[digit] = glyph(dots, size)
    fb = FontBuilder(EM, isTTF=True)
    fb.setupGlyphOrder(order)
    cmap = {code: g for g, code in SEPARATORS.items()}
    cmap.update({ord(str(i)): DIGITS[i] for i in range(10)})
    fb.setupCharacterMap(cmap)
    fb.setupGlyf(glyphs)
    # Text places a glyph by its left side bearing, not by its own coordinates: it has to be the
    # glyph's xMin, or every frame slides left by its leftmost dot and the stacked layers drift apart.
    glyf = fb.font["glyf"]
    metrics = {g: (EM if g in DIGITS else 0, getattr(glyf[g], "xMin", 0)) for g in order}
    fb.setupHorizontalMetrics(metrics)
    fb.setupHorizontalHeader(ascent=EM, descent=0)
    fb.setupOS2(sTypoAscender=EM, sTypoDescender=0, sTypoLineGap=0, usWinAscent=EM, usWinDescent=0)
    fb.setupNameTable(names(name))
    fb.setupPost()
    fb.addOpenTypeFeatures(FEATURES)
    fb.save(out / f"{name}.ttf")


def main():
    frames_file, out_dir = Path(sys.argv[1]), Path(sys.argv[2])
    data = json.loads(frames_file.read_text())
    out_dir.mkdir(parents=True, exist_ok=True)
    for name, frames in sorted(data["fonts"].items()):
        assert len(frames) == 10, f"{name}: a timer digit needs 10 frames"
        build(name, frames, data["size"], out_dir)
    print(f"wrote {len(data['fonts'])} fonts to {out_dir}")
    validate(out_dir)


# The same check App Store ingestion runs on every font (TN3214); WidgetKit apps get `strict`.
FONT_VALIDATOR = ("/System/Library/Frameworks/ApplicationServices.framework/Frameworks/"
                  "ATS.framework/Support/FontValidator")


def validate(path):
    """Fails unless every font under `path` passes FontValidator, before it reaches App Review."""
    if not Path(FONT_VALIDATOR).exists():
        sys.exit("FontValidator is macOS-only: run this on a Mac so the fonts get validated")
    # FontValidator 3.5 (macOS 27.2) takes the App Store's options; older ones exit 252 on them.
    for options in (["-platform", "iOS", "-level", "strict", "-reportType", "detailed"], ["-ios_only", "-report"]):
        result = subprocess.run([FONT_VALIDATOR, *options, str(path)], capture_output=True, text=True)
        if result.returncode != 252:
            break
    if result.returncode != 0:
        sys.exit(f"FontValidator rejected the fonts:\n{result.stdout}{result.stderr}")
    print("FontValidator: all fonts pass")


if __name__ == "__main__":
    main()
