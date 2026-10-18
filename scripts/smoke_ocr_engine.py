"""Run an offline JSONL smoke using binary pipes, independent of shell encoding."""
import argparse
import base64
import json
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("engine")
    parser.add_argument("models")
    args = parser.parse_args()
    request = {"pixels_b64": base64.b64encode(bytes(768)).decode("ascii"), "width": 16, "height": 16}
    result = subprocess.run(
        [args.engine, args.models, "1", "cpu"],
        input=(json.dumps(request) + "\n").encode("utf-8"),
        capture_output=True, timeout=120, check=True,
    )
    response = json.loads(result.stdout)
    if response.get("status") != "ok" or response.get("provider") != "cpu":
        raise RuntimeError(f"Unexpected Rust OCR response: {response}")
    print(json.dumps(response))


if __name__ == "__main__":
    main()
