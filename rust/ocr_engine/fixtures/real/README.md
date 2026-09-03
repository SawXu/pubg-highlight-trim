# Real PUBG OCR benchmark

This fixture contains twelve ROI crops extracted from the local PUBG highlight recordings used by the `pubg-self-knock-elim-trimmer` skill. The crops are taken at the event timestamps recorded by the skill's `检测与裁剪记录.csv` files, using the production ROI `(0.30, 0.66, 0.70, 0.75)`.

`baseline.jsonl` stores the source recording name, event timestamp, language classification, and the Python/PaddleOCR text annotation captured in the skill record. The source videos are intentionally not redistributed; `scripts/build_real_ocr_benchmark.py` can recreate the crops when the local skill media directory is available.

The fixture covers four `zh-Hans`, four `zh-Hant`, and four English samples across self-death and own-kill events.
