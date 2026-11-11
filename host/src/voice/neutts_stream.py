"""Source: Hermes tools/neutts_synth.py (MIT, 10d25f8a), NeuTTS 1.4.1 infer_stream.
Adaptation: cache-only, bounded PCM frames on stdout; model chatter goes to stderr.
"""
import base64
import contextlib
import importlib.metadata
import json
from pathlib import Path
import sys

protocol = sys.stdout
if not (3, 11) <= sys.version_info[:2] <= (3, 13):
    raise RuntimeError("NeuTTS requires Python 3.11–3.13")
root = Path(sys.argv[1])
text = json.loads(sys.stdin.buffer.read(64 * 1024)).get("text")
if not isinstance(text, str):
    raise ValueError("Invalid speech payload")
if not text.strip() or len(text.encode("utf-8")) > 4000 or "\0" in text:
    raise ValueError("Invalid speech text")
with contextlib.redirect_stdout(sys.stderr):
    if importlib.metadata.version("neutts") != "1.4.1":
        raise RuntimeError("Use Hermes's pinned NeuTTS 1.4.1 environment")
    import numpy as np
    from neutts import NeuTTS
    reference = root / "tools" / "neutts_samples"
    tts = NeuTTS(backbone_repo="neuphonic/neutts-air-q4-gguf", backbone_device="cpu",
                 codec_repo="neuphonic/neucodec", codec_device="cpu")
    codes = tts.encode_reference(str(reference / "jo.wav"))
    words = (reference / "jo.txt").read_text(encoding="utf-8-sig").strip()
    total = 0
    for samples in tts.infer_stream(text, codes, words):
        samples = np.asarray(samples, dtype=np.float32).reshape(-1)
        if not np.isfinite(samples).all():
            raise RuntimeError("NeuTTS produced nonfinite samples")
        total += samples.size
        if total > 24000 * 80:
            raise RuntimeError("Speech exceeds eighty seconds")
        for offset in range(0, samples.size, 12000):
            pcm = (np.clip(samples[offset:offset + 12000], -1, 1) * 32767).astype("<i2").tobytes()
            if pcm:
                protocol.write(json.dumps({"pcm": base64.b64encode(pcm).decode("ascii")}) + "\n")
                protocol.flush()
protocol.write('{"done":true}\n')
protocol.flush()
